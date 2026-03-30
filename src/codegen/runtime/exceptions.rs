use crate::codegen::emit::Emitter;

pub fn emit_exception_cleanup_frames(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exception_cleanup_frames ---");
    emitter.label("__rt_exception_cleanup_frames");

    // -- save callee-saved state used by the cleanup walk --
    emitter.instruction("sub sp, sp, #48");                                      // reserve stack space for x19/x20 plus frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                              // save frame pointer and return address for the cleanup walker
    emitter.instruction("stp x19, x20, [sp, #16]");                              // preserve callee-saved registers that track the walk state
    emitter.instruction("add x29, sp, #32");                                     // install the cleanup walker's frame pointer
    emitter.instruction("mov x19, x0");                                          // x19 = activation record that should remain on top after unwinding
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                    // load page of the call-frame stack top
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");              // resolve the call-frame stack top address
    emitter.instruction("ldr x20, [x9]");                                        // x20 = current activation record being examined

    // -- walk and clean every activation above the target stop frame --
    emitter.label("__rt_exception_cleanup_frames_loop");
    emitter.instruction("cmp x20, x19");                                         // have we reached the activation record that should survive the catch?
    emitter.instruction("b.eq __rt_exception_cleanup_frames_done");               // stop once the surviving activation is on top
    emitter.instruction("cbz x20, __rt_exception_cleanup_frames_done");           // stop defensively if the stack unexpectedly bottoms out
    emitter.instruction("ldr x10, [x20, #8]");                                   // load the cleanup callback pointer for this activation
    emitter.instruction("ldr x11, [x20, #16]");                                  // load the saved frame pointer for this activation
    emitter.instruction("cbz x10, __rt_exception_cleanup_frames_next");           // skip callbacks for activations that have no cleanup work
    emitter.instruction("mov x0, x11");                                          // pass the unwound activation's frame pointer to its cleanup callback
    emitter.instruction("blr x10");                                              // run the per-function cleanup callback for this activation

    emitter.label("__rt_exception_cleanup_frames_next");
    emitter.instruction("ldr x20, [x20]");                                       // advance to the previous activation record in the cleanup stack
    emitter.instruction("b __rt_exception_cleanup_frames_loop");                  // continue unwinding older activations until the target is reached

    // -- publish the surviving activation record as the new top --
    emitter.label("__rt_exception_cleanup_frames_done");
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                    // reload page of the call-frame stack top after callback calls
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");              // resolve the call-frame stack top address again
    emitter.instruction("str x19, [x9]");                                        // store the surviving activation record as the new call-frame top
    emitter.instruction("ldp x19, x20, [sp, #16]");                              // restore the callee-saved walk-state registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                      // release the cleanup walker's stack frame
    emitter.instruction("ret");                                                  // return to the throw helper after unwound-frame cleanup
}

pub fn emit_exception_matches(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exception_matches ---");
    emitter.label("__rt_exception_matches");

    // -- null exceptions never match any catch type --
    emitter.instruction("cbz x0, __rt_exception_matches_no");                     // null means there is no active exception object to test
    emitter.instruction("ldr x9, [x0]");                                         // x9 = runtime class_id stored in the thrown object header
    emitter.instruction("cbnz x2, __rt_exception_matches_interface");             // x2 != 0 means this catch target is an interface, not a class
    emitter.instruction("adrp x10, _class_gc_desc_count@PAGE");                  // load page of the emitted class-count table
    emitter.instruction("add x10, x10, _class_gc_desc_count@PAGEOFF");           // resolve the emitted class-count table address
    emitter.instruction("ldr x10, [x10]");                                       // x10 = total number of emitted classes
    emitter.instruction("adrp x11, _class_parent_ids@PAGE");                     // load page of the runtime parent-id table
    emitter.instruction("add x11, x11, _class_parent_ids@PAGEOFF");              // resolve the runtime parent-id table address
    emitter.instruction("mov x12, #-1");                                         // x12 = sentinel parent id used for root classes

    // -- walk parent links until we either match or hit the root --
    emitter.label("__rt_exception_matches_loop");
    emitter.instruction("cmp x9, x1");                                           // does the current class_id equal the catch class_id?
    emitter.instruction("b.eq __rt_exception_matches_yes");                       // matching class ids mean the catch clause applies
    emitter.instruction("cmp x9, x10");                                          // is the current class_id outside the emitted class table?
    emitter.instruction("b.hs __rt_exception_matches_no");                        // out-of-range ids cannot match any catch type safely
    emitter.instruction("lsl x13, x9, #3");                                      // scale class_id by 8 bytes per parent-id entry
    emitter.instruction("ldr x9, [x11, x13]");                                   // follow the current class's parent class_id link
    emitter.instruction("cmp x9, x12");                                          // have we reached a class with no parent?
    emitter.instruction("b.eq __rt_exception_matches_no");                        // root reached without a match means this catch does not apply
    emitter.instruction("b __rt_exception_matches_loop");                         // continue walking up the inheritance chain

    // -- interface catch: scan the class's emitted interface id list --
    emitter.label("__rt_exception_matches_interface");
    emitter.instruction("adrp x10, _class_gc_desc_count@PAGE");                  // load page of the emitted class-count table
    emitter.instruction("add x10, x10, _class_gc_desc_count@PAGEOFF");           // resolve the emitted class-count table address
    emitter.instruction("ldr x10, [x10]");                                       // x10 = total number of emitted classes
    emitter.instruction("cmp x9, x10");                                          // is the thrown object's class_id within the emitted class table?
    emitter.instruction("b.hs __rt_exception_matches_no");                        // out-of-range class ids cannot satisfy an interface catch safely
    emitter.instruction("adrp x11, _class_interface_ptrs@PAGE");                 // load page of the class-to-interface metadata table
    emitter.instruction("add x11, x11, _class_interface_ptrs@PAGEOFF");          // resolve the class-to-interface metadata table address
    emitter.instruction("lsl x12, x9, #3");                                      // scale class_id by 8 bytes per metadata pointer
    emitter.instruction("ldr x11, [x11, x12]");                                  // x11 = pointer to this class's interface metadata block
    emitter.instruction("ldr x12, [x11]");                                       // x12 = number of emitted interfaces for this class
    emitter.instruction("add x11, x11, #8");                                     // advance x11 to the first [interface_id, impl_ptr] pair

    emitter.label("__rt_exception_matches_interface_loop");
    emitter.instruction("cbz x12, __rt_exception_matches_no");                    // no remaining interfaces means this catch target does not apply
    emitter.instruction("ldr x13, [x11]");                                       // x13 = current implemented interface_id
    emitter.instruction("cmp x13, x1");                                          // does this implemented interface match the catch target id?
    emitter.instruction("b.eq __rt_exception_matches_yes");                       // matching interface ids mean the catch clause applies
    emitter.instruction("add x11, x11, #16");                                    // advance to the next [interface_id, impl_ptr] pair
    emitter.instruction("sub x12, x12, #1");                                     // consume one implemented interface entry
    emitter.instruction("b __rt_exception_matches_interface_loop");               // continue scanning the emitted interface list

    emitter.label("__rt_exception_matches_yes");
    emitter.instruction("mov x0, #1");                                           // return true when the thrown object is an instance of the catch type
    emitter.instruction("ret");                                                  // finish the instanceof-style catch test

    emitter.label("__rt_exception_matches_no");
    emitter.instruction("mov x0, #0");                                           // return false when the catch type does not match the thrown object
    emitter.instruction("ret");                                                  // finish the instanceof-style catch test
}

pub fn emit_throw_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: throw_current ---");
    emitter.label("__rt_throw_current");

    // -- save callee-saved state while the throw helper inspects handler stacks --
    emitter.instruction("sub sp, sp, #48");                                      // reserve stack space for handler state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                              // save frame pointer and return address for the throw helper
    emitter.instruction("stp x19, x20, [sp, #16]");                              // preserve callee-saved registers that hold handler metadata
    emitter.instruction("add x29, sp, #32");                                     // install the throw helper's frame pointer
    emitter.instruction("adrp x9, _exc_handler_top@PAGE");                       // load page of the exception-handler stack top
    emitter.instruction("add x9, x9, _exc_handler_top@PAGEOFF");                 // resolve the exception-handler stack top address
    emitter.instruction("ldr x19, [x9]");                                        // x19 = current top-most exception handler
    emitter.instruction("cbz x19, __rt_throw_current_uncaught");                 // fall back to a fatal uncaught-exception path when no handler exists
    emitter.instruction("ldr x0, [x19, #8]");                                    // x0 = activation record that should survive this catch
    emitter.instruction("bl __rt_exception_cleanup_frames");                     // run cleanup callbacks for every unwound activation frame
    emitter.instruction("adrp x9, _concat_off@PAGE");                            // load page of the concat cursor before resuming via longjmp
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                      // resolve the concat cursor address
    emitter.instruction("str xzr, [x9]");                                        // clear any partially-built concat state before catch/finally code resumes
    emitter.instruction("add x0, x19, #16");                                     // x0 = jmp_buf base stored inside the active handler record
    emitter.instruction("mov x1, #1");                                           // longjmp return value = 1 to indicate exceptional control flow
    emitter.instruction("bl _longjmp");                                          // transfer control directly back to the saved catch resume point

    // -- uncaught exceptions terminate the process with a fatal message --
    emitter.label("__rt_throw_current_uncaught");
    emitter.instruction("adrp x1, _uncaught_exc_msg@PAGE");                      // load page of the uncaught-exception error message
    emitter.instruction("add x1, x1, _uncaught_exc_msg@PAGEOFF");                // resolve the uncaught-exception error message address
    emitter.instruction("mov x2, #32");                                          // uncaught exception message length in bytes
    emitter.instruction("mov x0, #2");                                           // fd = stderr for fatal runtime diagnostics
    emitter.instruction("mov x16, #4");                                          // syscall 4 = write on macOS
    emitter.instruction("svc #0x80");                                            // print the fatal uncaught-exception message
    emitter.instruction("mov x0, #1");                                           // exit status 1 indicates abnormal termination
    emitter.instruction("mov x16, #1");                                          // syscall 1 = exit on macOS
    emitter.instruction("svc #0x80");                                            // terminate immediately after an uncaught exception
}

pub fn emit_rethrow_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rethrow_current ---");
    emitter.label("__rt_rethrow_current");
    emitter.instruction("b __rt_throw_current");                                 // re-use the ordinary throw helper with the existing active exception state
}
