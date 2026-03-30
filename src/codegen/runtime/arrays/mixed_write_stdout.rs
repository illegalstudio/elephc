use crate::codegen::emit::Emitter;

pub fn emit_mixed_write_stdout(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_write_stdout ---");
    emitter.label("__rt_mixed_write_stdout");

    emitter.instruction("sub sp, sp, #16");                                       // allocate a small frame so nested helper calls can preserve x30
    emitter.instruction("str x30, [sp]");                                         // save the caller return address before any nested bl instructions
    emitter.instruction("cbz x0, __rt_mixed_write_stdout_done");                  // null mixed pointers print nothing
    emitter.instruction("ldr x9, [x0]");                                          // load the boxed runtime payload tag
    emitter.instruction("cmp x9, #8");                                            // is the boxed value null?
    emitter.instruction("b.eq __rt_mixed_write_stdout_done");                     // null prints nothing
    emitter.instruction("cmp x9, #3");                                            // is the boxed value a bool?
    emitter.instruction("b.eq __rt_mixed_write_stdout_bool");                     // booleans need PHP echo semantics
    emitter.instruction("cmp x9, #0");                                            // is the boxed value an integer?
    emitter.instruction("b.eq __rt_mixed_write_stdout_int");                      // integers print via itoa
    emitter.instruction("cmp x9, #2");                                            // is the boxed value a float?
    emitter.instruction("b.eq __rt_mixed_write_stdout_float");                    // floats print via ftoa
    emitter.instruction("cmp x9, #1");                                            // is the boxed value a string?
    emitter.instruction("b.ne __rt_mixed_write_stdout_done");                     // non-scalar boxed payloads print nothing for echo
    emitter.instruction("ldr x1, [x0, #8]");                                      // load the boxed string pointer
    emitter.instruction("ldr x2, [x0, #16]");                                     // load the boxed string length
    emitter.instruction("mov x0, #1");                                            // fd = stdout
    emitter.instruction("mov x16, #4");                                           // syscall 4 = write
    emitter.instruction("svc #0x80");                                             // invoke macOS kernel to print the boxed string
    emitter.instruction("b __rt_mixed_write_stdout_done");                        // restore x30 and return after printing the boxed string

    emitter.label("__rt_mixed_write_stdout_bool");
    emitter.instruction("ldr x0, [x0, #8]");                                      // load the boxed bool payload
    emitter.instruction("cbz x0, __rt_mixed_write_stdout_done");                  // false prints an empty string
    emitter.instruction("bl __rt_itoa");                                          // true prints as integer 1
    emitter.instruction("mov x0, #1");                                            // fd = stdout
    emitter.instruction("mov x16, #4");                                           // syscall 4 = write
    emitter.instruction("svc #0x80");                                             // invoke macOS kernel to print the boxed bool
    emitter.instruction("b __rt_mixed_write_stdout_done");                        // restore x30 and return after printing the boxed bool

    emitter.label("__rt_mixed_write_stdout_int");
    emitter.instruction("ldr x0, [x0, #8]");                                      // load the boxed integer payload
    emitter.instruction("bl __rt_itoa");                                          // convert the boxed integer to a decimal string
    emitter.instruction("mov x0, #1");                                            // fd = stdout
    emitter.instruction("mov x16, #4");                                           // syscall 4 = write
    emitter.instruction("svc #0x80");                                             // invoke macOS kernel to print the boxed integer
    emitter.instruction("b __rt_mixed_write_stdout_done");                        // restore x30 and return after printing the boxed integer

    emitter.label("__rt_mixed_write_stdout_float");
    emitter.instruction("ldr x9, [x0, #8]");                                      // load the boxed float bits
    emitter.instruction("fmov d0, x9");                                           // move the boxed float bits into the FP return register
    emitter.instruction("bl __rt_ftoa");                                          // convert the boxed float to a printable string
    emitter.instruction("mov x0, #1");                                            // fd = stdout
    emitter.instruction("mov x16, #4");                                           // syscall 4 = write
    emitter.instruction("svc #0x80");                                             // invoke macOS kernel to print the boxed float
    emitter.instruction("b __rt_mixed_write_stdout_done");                        // restore x30 and return after printing the boxed float

    emitter.label("__rt_mixed_write_stdout_done");
    emitter.instruction("ldr x30, [sp]");                                         // restore the caller return address after any nested helper calls
    emitter.instruction("add sp, sp, #16");                                       // deallocate the mixed-write frame
    emitter.instruction("ret");                                                   // return to the caller
}
