use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_push_int: push an integer element to an array, growing if needed.
/// Input:  x0 = array pointer, x1 = value
/// Output: x0 = array pointer (may differ if array was reallocated)
pub fn emit_array_push_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label_global("__rt_array_push_int");

    // -- split shared arrays before appending in place --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the appended value across ensure_unique
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared arrays before the append path mutates storage
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore the appended value after ensure_unique

    // -- check capacity before pushing --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array capacity
    emitter.instruction("cmp x9, x10");                                         // is the array full?
    emitter.instruction("b.ge __rt_array_push_int_grow");                       // grow array if at capacity

    // -- fast path: push directly --
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8] (8 bytes per int)
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return to caller (x0 unchanged)

    // -- slow path: grow array then push --
    emitter.label("__rt_array_push_int_grow");
    emitter.instruction("bl __rt_array_grow");                                  // double array capacity → x0 = new array

    emitter.instruction("ldr x1, [sp, #0]");                                    // restore value to push
    emitter.instruction("ldr x9, [x0]");                                        // reload length from new array
    emitter.instruction("add x10, x0, #24");                                    // x10 = data region of new array
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8]
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // update length in new array

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return with x0 = new array
}

fn emit_array_push_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label_global("__rt_array_push_int");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving indexed-array append spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved scalar payload and array pointer
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the appended scalar payload and the possibly-grown array pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the appended scalar payload across uniqueness and growth helper calls
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before appending a new scalar slot
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the unique indexed-array pointer across the optional growth helper call
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the indexed-array logical length before checking the append capacity
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the indexed-array capacity before deciding between the fast path and growth
    emitter.instruction("cmp r10, r11");                                        // is the indexed array already full at the current logical length?
    emitter.instruction("jae __rt_array_push_int_grow");                        // grow the indexed array when the new element would exceed the current capacity
    emitter.label("__rt_array_push_int_store");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the current indexed-array pointer before writing the appended scalar slot
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // reload the indexed-array logical length after helper calls clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the appended scalar payload after helper calls clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax + 24 + r10 * 8], r11");             // store the appended scalar payload into the next indexed-array slot
    emitter.instruction("add r10, 1");                                          // advance the indexed-array logical length after materializing the appended slot
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the updated indexed-array logical length in the array header
    emitter.instruction("add rsp, 16");                                         // release the indexed-array append spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the updated indexed array
    emitter.instruction("ret");                                                 // return to the caller with rax holding the updated indexed-array pointer
    emitter.label("__rt_array_push_int_grow");
    emitter.instruction("mov rdi, rax");                                        // pass the unique indexed-array pointer to the growth helper before appending the new scalar slot
    emitter.instruction("call __rt_array_grow");                                // allocate a larger indexed-array backing store so the append can proceed
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the grown indexed-array pointer before writing the appended scalar slot
    emitter.instruction("jmp __rt_array_push_int_store");                       // append the scalar payload into the grown indexed-array storage
}
