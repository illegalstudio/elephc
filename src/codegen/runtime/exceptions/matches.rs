use crate::codegen::emit::Emitter;

pub fn emit_exception_matches(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exception_matches ---");
    emitter.label_global("__rt_exception_matches");

    // -- null exceptions never match any catch type --
    emitter.instruction("cbz x0, __rt_exception_matches_no");                   // null means there is no active exception object to test
    emitter.instruction("ldr x9, [x0]");                                        // x9 = runtime class_id stored in the thrown object header
    emitter.instruction("cbnz x2, __rt_exception_matches_interface");           // x2 != 0 means this catch target is an interface, not a class
    emitter.instruction("adrp x10, _class_gc_desc_count@PAGE");                 // load page of the emitted class-count table
    emitter.instruction("add x10, x10, _class_gc_desc_count@PAGEOFF");          // resolve the emitted class-count table address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = total number of emitted classes
    emitter.instruction("adrp x11, _class_parent_ids@PAGE");                    // load page of the runtime parent-id table
    emitter.instruction("add x11, x11, _class_parent_ids@PAGEOFF");             // resolve the runtime parent-id table address
    emitter.instruction("mov x12, #-1");                                        // x12 = sentinel parent id used for root classes

    // -- walk parent links until we either match or hit the root --
    emitter.label("__rt_exception_matches_loop");
    emitter.instruction("cmp x9, x1");                                          // does the current class_id equal the catch class_id?
    emitter.instruction("b.eq __rt_exception_matches_yes");                     // matching class ids mean the catch clause applies
    emitter.instruction("cmp x9, x10");                                         // is the current class_id outside the emitted class table?
    emitter.instruction("b.hs __rt_exception_matches_no");                      // out-of-range ids cannot match any catch type safely
    emitter.instruction("lsl x13, x9, #3");                                     // scale class_id by 8 bytes per parent-id entry
    emitter.instruction("ldr x9, [x11, x13]");                                  // follow the current class's parent class_id link
    emitter.instruction("cmp x9, x12");                                         // have we reached a class with no parent?
    emitter.instruction("b.eq __rt_exception_matches_no");                      // root reached without a match means this catch does not apply
    emitter.instruction("b __rt_exception_matches_loop");                       // continue walking up the inheritance chain

    // -- interface catch: scan the class's emitted interface id list --
    emitter.label("__rt_exception_matches_interface");
    emitter.instruction("adrp x10, _class_gc_desc_count@PAGE");                 // load page of the emitted class-count table
    emitter.instruction("add x10, x10, _class_gc_desc_count@PAGEOFF");          // resolve the emitted class-count table address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = total number of emitted classes
    emitter.instruction("cmp x9, x10");                                         // is the thrown object's class_id within the emitted class table?
    emitter.instruction("b.hs __rt_exception_matches_no");                      // out-of-range class ids cannot satisfy an interface catch safely
    emitter.instruction("adrp x11, _class_interface_ptrs@PAGE");                // load page of the class-to-interface metadata table
    emitter.instruction("add x11, x11, _class_interface_ptrs@PAGEOFF");         // resolve the class-to-interface metadata table address
    emitter.instruction("lsl x12, x9, #3");                                     // scale class_id by 8 bytes per metadata pointer
    emitter.instruction("ldr x11, [x11, x12]");                                 // x11 = pointer to this class's interface metadata block
    emitter.instruction("ldr x12, [x11]");                                      // x12 = number of emitted interfaces for this class
    emitter.instruction("add x11, x11, #8");                                    // advance x11 to the first [interface_id, impl_ptr] pair

    emitter.label("__rt_exception_matches_interface_loop");
    emitter.instruction("cbz x12, __rt_exception_matches_no");                  // no remaining interfaces means this catch target does not apply
    emitter.instruction("ldr x13, [x11]");                                      // x13 = current implemented interface_id
    emitter.instruction("cmp x13, x1");                                         // does this implemented interface match the catch target id?
    emitter.instruction("b.eq __rt_exception_matches_yes");                     // matching interface ids mean the catch clause applies
    emitter.instruction("add x11, x11, #16");                                   // advance to the next [interface_id, impl_ptr] pair
    emitter.instruction("sub x12, x12, #1");                                    // consume one implemented interface entry
    emitter.instruction("b __rt_exception_matches_interface_loop");             // continue scanning the emitted interface list

    emitter.label("__rt_exception_matches_yes");
    emitter.instruction("mov x0, #1");                                          // return true when the thrown object is an instance of the catch type
    emitter.instruction("ret");                                                 // finish the instanceof-style catch test

    emitter.label("__rt_exception_matches_no");
    emitter.instruction("mov x0, #0");                                          // return false when the catch type does not match the thrown object
    emitter.instruction("ret");                                                 // finish the instanceof-style catch test
}
