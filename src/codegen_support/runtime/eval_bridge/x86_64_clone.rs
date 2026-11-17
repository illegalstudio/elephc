//! Purpose:
//! Emits the x86_64 boxed-object shallow-clone wrapper.
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

/// Emits the x86_64 native wrapper for cloning boxed object cells.
pub(super) fn emit_x86_64_object_clone_shallow_wrapper(emitter: &mut Emitter, native_validation: bool) {
    emitter.label_global("__rt_object_clone_shallow_boxed");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across clone calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable clone wrapper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve source, clone, descriptor, counters, and payload slots
    emitter.instruction("test rdi, rdi");                                       // null handles cannot be cloned as objects
    emitter.instruction("jz __rt_object_clone_shallow_boxed_null_x86");         // branch to the null sentinel for null handles
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 = object
    emitter.instruction("jne __rt_object_clone_shallow_boxed_null_x86");        // non-object values cannot be cloned by this bridge
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__rt_object_clone_shallow_boxed_null_x86",
    );
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the source object payload pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the object's runtime class id
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save class id across allocation and ownership calls
    emit_x86_64_reject_runtime_managed_clone_classes(emitter, "rax", "__rt_object_clone_shallow_boxed_null_x86");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_class_gc_desc_count", 0);
    emitter.instruction("cmp rax, r11");                                        // is this class id inside the descriptor table?
    emitter.instruction("jae __rt_object_clone_shallow_boxed_null_x86");        // unknown class layouts cannot be cloned by the eval bridge
    abi::emit_symbol_address(emitter, "r11", "_class_gc_desc_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + rax * 8]");                  // load the class property-tag descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save descriptor pointer for the property-copy loop
    abi::emit_symbol_address(emitter, "r11", "_class_object_payload_sizes");
    emitter.instruction("mov rcx, QWORD PTR [r11 + rax * 8]");                  // load the class-declared object payload size
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save payload size for allocation and dyn-prop detection
    abi::emit_symbol_address(emitter, "r11", "_class_object_allocation_sizes");
    emitter.instruction("mov rax, QWORD PTR [r11 + rax * 8]");                  // include ordinary Throwable construction metadata in the allocation
    emitter.instruction("call __rt_heap_alloc");                                // allocate a clone object payload with the same byte size
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(4))); // materialize the x86_64 object heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform object heap header
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the new object to its PHP object handle
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the source class id
    emitter.instruction("mov QWORD PTR [rax], rcx");                            // store the class id at the clone payload head
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the clone object payload pointer
    abi::emit_symbol_address(emitter, "r11", "_class_throwable_line_offsets");
    emitter.instruction("mov r11, QWORD PTR [r11 + rcx * 8]");                  // locate optional ordinary Throwable construction metadata
    emitter.instruction("test r11, r11");                                       // ordinary classes do not carry Throwable metadata
    emitter.instruction("jz __rt_clone_line_copied_x86");                       // skip the metadata copy for non-Throwable classes
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source object independently of its physical layout
    super::super::exceptions::emit_creation_line_load(emitter, "r8", "rax", "r10", "__rt_clone_source");
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // recover the clone pointer after reading the source line
    emitter.instruction("mov QWORD PTR [r8 + r11], rax");                       // copy metadata outside the clone's PHP property region
    emitter.label("__rt_clone_line_copied_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the payload size
    emitter.instruction("sub r10, 8");                                          // remove the leading class id field from the clone layout
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the source class id for dynamic-property metadata
    abi::emit_symbol_address(emitter, "r11", "_class_object_dynamic_prop_flags");
    emitter.instruction("mov r11, QWORD PTR [r11 + rax * 8]");                  // load whether this class layout has a dyn-props tail
    emitter.instruction("test r11, r11");                                       // check whether dyn-props bytes should be excluded
    emitter.instruction("jz __rt_object_clone_shallow_boxed_count_ready_x86");  // no dyn-props tail contributes to property count
    emitter.instruction("sub r10, 8");                                          // remove the dyn-props tail before counting declared slots
    emitter.label("__rt_object_clone_shallow_boxed_count_ready_x86");
    emitter.instruction("shr r10, 4");                                          // derive declared-property slot count from the payload size
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save property count for the copy loop
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize property-copy index to zero

    elephc_memoir::values::object_clone_slots::emit_tagged_slots(emitter);

    emitter.label("__rt_object_clone_shallow_boxed_dyn_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the source class id for dynamic-property metadata
    abi::emit_symbol_address(emitter, "r11", "_class_object_dynamic_prop_flags");
    emitter.instruction("mov r11, QWORD PTR [r11 + rax * 8]");                  // load whether this class layout has a dyn-props tail
    emitter.instruction("test r11, r11");                                       // check whether a dyn-props hash slot exists
    emitter.instruction("jz __rt_object_clone_shallow_boxed_box_x86");          // no dynamic hash slot: box the copied clone
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the class-declared payload size
    emitter.instruction("sub r10, 8");                                          // compute dyn-props slot offset as payload_size - 8
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save dyn-props slot offset across hash cloning
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source object pointer
    emitter.instruction("mov rax, QWORD PTR [r11 + r10]");                      // load the source dynamic-property hash pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the clone object pointer
    emitter.instruction("test rax, rax");                                       // is the source dynamic-property hash present?
    emitter.instruction("jz __rt_object_clone_shallow_boxed_dyn_null_x86");     // null source hash stays null on the clone
    emitter.instruction("mov rdi, rax");                                        // pass the source dynamic hash to the clone helper
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone dynamic properties and retain nested values
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // restore the dynamic-property slot offset
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the clone object pointer after hash cloning
    emitter.instruction("mov QWORD PTR [r11 + r10], rax");                      // install the cloned dynamic-property hash
    emitter.instruction("jmp __rt_object_clone_shallow_boxed_box_x86");         // box the clone after dynamic properties are installed

    emitter.label("__rt_object_clone_shallow_boxed_dyn_null_x86");
    emitter.instruction("mov QWORD PTR [r11 + r10], 0");                        // clear the clone's dynamic-property hash slot

    emitter.label("__rt_object_clone_shallow_boxed_box_x86");
    if native_validation {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // register the completed clone before exposing its owning box
        abi::emit_call_label(emitter, "__rt_reference_sources_bind_object");
    }
    emitter.instruction("mov rax, 24");                                         // a Mixed cell stores a runtime tag and two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the Mixed cell that will own the fresh clone
    emitter.instruction(&x86_64_mixed_heap_kind_instruction());                 // materialize the Mixed-cell heap kind marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform Mixed-cell heap header
    emitter.instruction("mov QWORD PTR [rax], 6");                              // runtime tag 6 = object
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the clone, which still holds the allocator's only reference
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // move that reference into the Mixed cell rather than retaining a second one
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // object payloads do not use a high word
    emitter.instruction("jmp __rt_object_clone_shallow_boxed_done_x86");        // skip the null sentinel after a successful clone

    emitter.label("__rt_object_clone_shallow_boxed_null_x86");
    emitter.instruction("xor eax, eax");                                        // return a null C pointer for unsupported clone inputs
    emitter.label("__rt_object_clone_shallow_boxed_done_x86");
    emitter.instruction("add rsp, 64");                                         // release clone wrapper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed clone or null failure sentinel
}
