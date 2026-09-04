//! Purpose:
//! Emits the x86_64 boxed-object shallow-clone wrapper.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Runtime-managed payload rejection and Mixed boxing preserve ownership.

use super::*;

/// Emits the x86_64 eval bridge wrapper for cloning boxed object cells.
pub(super) fn emit_x86_64_object_clone_shallow_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_object_clone_shallow");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across clone calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable clone wrapper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve source, clone, descriptor, counters, and payload slots
    emitter.instruction("test rdi, rdi");                                       // null handles cannot be cloned as objects
    emitter.instruction("jz __elephc_eval_value_object_clone_shallow_null_x86"); // branch to the null sentinel for null handles
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 = object
    emitter.instruction("jne __elephc_eval_value_object_clone_shallow_null_x86"); // non-object values cannot be cloned by this bridge
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_object_clone_shallow_null_x86",
    );
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the source object payload pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the object's runtime class id
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save class id across allocation and ownership calls
    emit_x86_64_reject_runtime_managed_clone_classes(emitter, "rax", "__elephc_eval_value_object_clone_shallow_null_x86");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_class_gc_desc_count", 0);
    emitter.instruction("cmp rax, r11");                                        // is this class id inside the descriptor table?
    emitter.instruction("jae __elephc_eval_value_object_clone_shallow_null_x86"); // unknown class layouts cannot be cloned by the eval bridge
    abi::emit_symbol_address(emitter, "r11", "_class_gc_desc_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + rax * 8]");                  // load the class property-tag descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save descriptor pointer for the property-copy loop
    abi::emit_symbol_address(emitter, "r11", "_class_object_payload_sizes");
    emitter.instruction("mov rcx, QWORD PTR [r11 + rax * 8]");                  // load the class-declared object payload size
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save payload size for allocation and dyn-prop detection
    emitter.instruction("mov rax, rcx");                                        // pass the source payload size to the heap allocator
    emitter.instruction("call __rt_heap_alloc");                                // allocate a clone object payload with the same byte size
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(4))); // materialize the x86_64 object heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform object heap header
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the new object to its PHP object handle
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the source class id
    emitter.instruction("mov QWORD PTR [rax], rcx");                            // store the class id at the clone payload head
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the clone object payload pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the payload size
    emitter.instruction("sub r10, 8");                                          // remove the leading class id field from the clone layout
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the source class id for dynamic-property metadata
    abi::emit_symbol_address(emitter, "r11", "_class_object_dynamic_prop_flags");
    emitter.instruction("mov r11, QWORD PTR [r11 + rax * 8]");                  // load whether this class layout has a dyn-props tail
    emitter.instruction("test r11, r11");                                       // check whether dyn-props bytes should be excluded
    emitter.instruction("jz __elephc_eval_value_object_clone_shallow_count_ready_x86"); // no dyn-props tail contributes to property count
    emitter.instruction("sub r10, 8");                                          // remove the dyn-props tail before counting declared slots
    emitter.label("__elephc_eval_value_object_clone_shallow_count_ready_x86");
    emitter.instruction("shr r10, 4");                                          // derive declared-property slot count from the payload size
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save property count for the copy loop
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize property-copy index to zero

    emitter.label("__elephc_eval_value_object_clone_shallow_prop_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current property index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // has every declared property slot been copied?
    emitter.instruction("jae __elephc_eval_value_object_clone_shallow_dyn_x86"); // move on to the optional dynamic-property hash
    emitter.instruction("mov rcx, r10");                                        // copy property index before scaling it into a byte offset
    emitter.instruction("shl rcx, 4");                                          // each declared-property slot is two 8-byte words
    emitter.instruction("add rcx, 8");                                          // skip the leading class id to reach this slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source object pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the clone object pointer
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                      // copy the source property low word
    emitter.instruction("mov rdx, QWORD PTR [r11 + rcx + 8]");                  // copy the source property high word
    emitter.instruction("mov QWORD PTR [r8 + rcx], rax");                       // store the property low word on the clone
    emitter.instruction("mov QWORD PTR [r8 + rcx + 8], rdx");                   // store the property high word on the clone
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property-tag descriptor pointer
    emitter.instruction("movzx r11, BYTE PTR [r9 + r10]");                      // load the compile-time ownership tag for this slot
    emitter.instruction("cmp r11, 1");                                          // does the slot hold an owned string payload?
    emitter.instruction("je __elephc_eval_value_object_clone_shallow_string_x86"); // string slots need an independent payload copy
    emitter.instruction("cmp r11, 4");                                          // does the slot hold a retained indexed-array payload?
    emitter.instruction("je __elephc_eval_value_object_clone_shallow_retain_x86"); // retained array slots need an extra owner reference
    emitter.instruction("cmp r11, 5");                                          // does the slot hold a retained associative-array payload?
    emitter.instruction("je __elephc_eval_value_object_clone_shallow_retain_x86"); // retained hash slots need an extra owner reference
    emitter.instruction("cmp r11, 6");                                          // does the slot hold a retained object payload?
    emitter.instruction("je __elephc_eval_value_object_clone_shallow_retain_x86"); // retained object slots need an extra owner reference
    emitter.instruction("cmp r11, 7");                                          // does the slot hold a retained boxed Mixed payload?
    emitter.instruction("je __elephc_eval_value_object_clone_shallow_retain_x86"); // retained Mixed slots need an extra owner reference
    emitter.instruction("jmp __elephc_eval_value_object_clone_shallow_next_x86"); // scalar slots are copied without ownership changes

    emitter.label("__elephc_eval_value_object_clone_shallow_string_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve property index across string persistence
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // preserve the low-word slot offset across the helper
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload for clone ownership
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // restore the low-word slot offset after persistence
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the clone object pointer after persistence
    emitter.instruction("mov QWORD PTR [r8 + rcx], rax");                       // install the persisted string pointer on the clone
    emitter.instruction("mov QWORD PTR [r8 + rcx + 8], rdx");                   // install the persisted string length on the clone
    emitter.instruction("jmp __elephc_eval_value_object_clone_shallow_next_x86"); // continue with the next declared property

    emitter.label("__elephc_eval_value_object_clone_shallow_retain_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve property index across the retain helper
    emitter.instruction("call __rt_incref");                                    // retain the shared property payload for the cloned object, which the copy above already left in rax where the helper reads it

    emitter.label("__elephc_eval_value_object_clone_shallow_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next declared-property slot
    emitter.instruction("jmp __elephc_eval_value_object_clone_shallow_prop_loop_x86"); // continue copying declared properties

    emitter.label("__elephc_eval_value_object_clone_shallow_dyn_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the source class id for dynamic-property metadata
    abi::emit_symbol_address(emitter, "r11", "_class_object_dynamic_prop_flags");
    emitter.instruction("mov r11, QWORD PTR [r11 + rax * 8]");                  // load whether this class layout has a dyn-props tail
    emitter.instruction("test r11, r11");                                       // check whether a dyn-props hash slot exists
    emitter.instruction("jz __elephc_eval_value_object_clone_shallow_box_x86"); // no dynamic hash slot: box the copied clone
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the class-declared payload size
    emitter.instruction("sub r10, 8");                                          // compute dyn-props slot offset as payload_size - 8
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save dyn-props slot offset across hash cloning
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source object pointer
    emitter.instruction("mov rax, QWORD PTR [r11 + r10]");                      // load the source dynamic-property hash pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the clone object pointer
    emitter.instruction("test rax, rax");                                       // is the source dynamic-property hash present?
    emitter.instruction("jz __elephc_eval_value_object_clone_shallow_dyn_null_x86"); // null source hash stays null on the clone
    emitter.instruction("mov rdi, rax");                                        // pass the source dynamic hash to the clone helper
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone dynamic properties and retain nested values
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // restore the dynamic-property slot offset
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the clone object pointer after hash cloning
    emitter.instruction("mov QWORD PTR [r11 + r10], rax");                      // install the cloned dynamic-property hash
    emitter.instruction("jmp __elephc_eval_value_object_clone_shallow_box_x86"); // box the clone after dynamic properties are installed

    emitter.label("__elephc_eval_value_object_clone_shallow_dyn_null_x86");
    emitter.instruction("mov QWORD PTR [r11 + r10], 0");                        // clear the clone's dynamic-property hash slot

    emitter.label("__elephc_eval_value_object_clone_shallow_box_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // move the cloned object pointer into the Mixed payload
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the cloned object for Rust
    emitter.instruction("jmp __elephc_eval_value_object_clone_shallow_done_x86"); // skip the null sentinel after a successful clone

    emitter.label("__elephc_eval_value_object_clone_shallow_null_x86");
    emitter.instruction("xor eax, eax");                                        // return a null C pointer for unsupported clone inputs
    emitter.label("__elephc_eval_value_object_clone_shallow_done_x86");
    emitter.instruction("add rsp, 64");                                         // release clone wrapper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed clone or null failure sentinel
}
