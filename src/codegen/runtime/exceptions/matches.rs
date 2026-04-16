use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_exception_matches(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_exception_matches_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: exception_matches ---");
    emitter.label_global("__rt_exception_matches");

    // -- null exceptions never match any catch type --
    emitter.instruction("cbz x0, __rt_exception_matches_no");                   // null means there is no active exception object to test
    emitter.instruction("ldr x9, [x0]");                                        // x9 = runtime class_id stored in the thrown object header
    emitter.instruction("cbnz x2, __rt_exception_matches_interface");           // x2 != 0 means this catch target is an interface, not a class
    emitter.adrp("x10", "_class_gc_desc_count");                 // load page of the emitted class-count table
    emitter.add_lo12("x10", "x10", "_class_gc_desc_count");          // resolve the emitted class-count table address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = total number of emitted classes
    emitter.adrp("x11", "_class_parent_ids");                    // load page of the runtime parent-id table
    emitter.add_lo12("x11", "x11", "_class_parent_ids");             // resolve the runtime parent-id table address
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
    emitter.adrp("x10", "_class_gc_desc_count");                 // load page of the emitted class-count table
    emitter.add_lo12("x10", "x10", "_class_gc_desc_count");          // resolve the emitted class-count table address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = total number of emitted classes
    emitter.instruction("cmp x9, x10");                                         // is the thrown object's class_id within the emitted class table?
    emitter.instruction("b.hs __rt_exception_matches_no");                      // out-of-range class ids cannot satisfy an interface catch safely
    emitter.adrp("x11", "_class_interface_ptrs");                // load page of the class-to-interface metadata table
    emitter.add_lo12("x11", "x11", "_class_interface_ptrs");         // resolve the class-to-interface metadata table address
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

fn emit_exception_matches_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exception_matches ---");
    emitter.label_global("__rt_exception_matches");

    emitter.instruction("test rdi, rdi");                                       // null exceptions never match any catch target
    emitter.instruction("je __rt_exception_matches_no");                        // there is no active exception object to test
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // r8 = runtime class_id stored in the thrown object header
    emitter.instruction("test rdx, rdx");                                       // does the catch target describe an interface instead of a class?
    emitter.instruction("jne __rt_exception_matches_interface");                // interface catches use the class-to-interface metadata table instead of parent links
    emitter.instruction("mov r9, QWORD PTR [rip + _class_gc_desc_count]");      // r9 = total number of emitted class metadata slots
    emitter.instruction("lea r10, [rip + _class_parent_ids]");                  // materialize the runtime parent-id table base pointer
    emitter.instruction("mov r11, -1");                                         // r11 = sentinel parent id used for root classes

    emitter.label("__rt_exception_matches_loop");
    emitter.instruction("cmp r8, rsi");                                         // does the current class_id equal the catch target class id?
    emitter.instruction("je __rt_exception_matches_yes");                       // matching class ids mean the catch clause applies
    emitter.instruction("cmp r8, r9");                                          // is the current class_id outside the emitted class table?
    emitter.instruction("jae __rt_exception_matches_no");                       // out-of-range ids cannot match any catch type safely
    emitter.instruction("mov r8, QWORD PTR [r10 + r8 * 8]");                    // follow the current class's parent class_id link
    emitter.instruction("cmp r8, r11");                                         // have we reached a class with no parent?
    emitter.instruction("je __rt_exception_matches_no");                        // root reached without a match means this catch does not apply
    emitter.instruction("jmp __rt_exception_matches_loop");                     // continue walking up the inheritance chain

    emitter.label("__rt_exception_matches_interface");
    emitter.instruction("mov r9, QWORD PTR [rip + _class_gc_desc_count]");      // r9 = total number of emitted class metadata slots
    emitter.instruction("cmp r8, r9");                                          // is the thrown object's class_id within the emitted class table?
    emitter.instruction("jae __rt_exception_matches_no");                       // out-of-range class ids cannot satisfy an interface catch safely
    emitter.instruction("lea r10, [rip + _class_interface_ptrs]");              // materialize the class-to-interface metadata table base pointer
    emitter.instruction("mov r10, QWORD PTR [r10 + r8 * 8]");                   // r10 = pointer to this class's interface metadata block
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // r11 = number of emitted interfaces for this class
    emitter.instruction("add r10, 8");                                          // advance to the first [interface_id, impl_ptr] pair

    emitter.label("__rt_exception_matches_interface_loop");
    emitter.instruction("test r11, r11");                                       // are there any remaining interfaces to scan for a catch match?
    emitter.instruction("je __rt_exception_matches_no");                        // no remaining interfaces means this catch target does not apply
    emitter.instruction("mov r12, QWORD PTR [r10]");                            // r12 = current implemented interface_id
    emitter.instruction("cmp r12, rsi");                                        // does this implemented interface match the catch target id?
    emitter.instruction("je __rt_exception_matches_yes");                       // matching interface ids mean the catch clause applies
    emitter.instruction("add r10, 16");                                         // advance to the next [interface_id, impl_ptr] pair
    emitter.instruction("sub r11, 1");                                          // consume one implemented interface entry
    emitter.instruction("jmp __rt_exception_matches_interface_loop");           // continue scanning the emitted interface list

    emitter.label("__rt_exception_matches_yes");
    emitter.instruction("mov eax, 1");                                          // return true when the thrown object is an instance of the catch type
    emitter.instruction("ret");                                                 // finish the instanceof-style catch test

    emitter.label("__rt_exception_matches_no");
    emitter.instruction("xor eax, eax");                                        // return false when the catch type does not match the thrown object
    emitter.instruction("ret");                                                 // finish the instanceof-style catch test
}
