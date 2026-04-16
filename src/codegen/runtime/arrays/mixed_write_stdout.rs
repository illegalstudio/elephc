use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_mixed_write_stdout(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_write_stdout_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_write_stdout ---");
    emitter.label_global("__rt_mixed_write_stdout");

    emitter.instruction("sub sp, sp, #16");                                     // allocate a small frame so nested helper calls can preserve x30
    emitter.instruction("str x30, [sp]");                                       // save the caller return address before any nested bl instructions
    emitter.instruction("cbz x0, __rt_mixed_write_stdout_done");                // null mixed pointers print nothing
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime payload tag
    emitter.instruction("cmp x9, #8");                                          // is the boxed value null?
    emitter.instruction("b.eq __rt_mixed_write_stdout_done");                   // null prints nothing
    emitter.instruction("cmp x9, #3");                                          // is the boxed value a bool?
    emitter.instruction("b.eq __rt_mixed_write_stdout_bool");                   // booleans need PHP echo semantics
    emitter.instruction("cmp x9, #0");                                          // is the boxed value an integer?
    emitter.instruction("b.eq __rt_mixed_write_stdout_int");                    // integers print via itoa
    emitter.instruction("cmp x9, #2");                                          // is the boxed value a float?
    emitter.instruction("b.eq __rt_mixed_write_stdout_float");                  // floats print via ftoa
    emitter.instruction("cmp x9, #1");                                          // is the boxed value a string?
    emitter.instruction("b.ne __rt_mixed_write_stdout_done");                   // non-scalar boxed payloads print nothing for echo
    emitter.instruction("ldr x1, [x0, #8]");                                    // load the boxed string pointer
    emitter.instruction("ldr x2, [x0, #16]");                                   // load the boxed string length
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("b __rt_mixed_write_stdout_done");                      // restore x30 and return after printing the boxed string

    emitter.label("__rt_mixed_write_stdout_bool");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed bool payload
    emitter.instruction("cbz x0, __rt_mixed_write_stdout_done");                // false prints an empty string
    emitter.instruction("bl __rt_itoa");                                        // true prints as integer 1
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("b __rt_mixed_write_stdout_done");                      // restore x30 and return after printing the boxed bool

    emitter.label("__rt_mixed_write_stdout_int");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed integer payload
    emitter.instruction("bl __rt_itoa");                                        // convert the boxed integer to a decimal string
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("b __rt_mixed_write_stdout_done");                      // restore x30 and return after printing the boxed integer

    emitter.label("__rt_mixed_write_stdout_float");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the boxed float bits
    emitter.instruction("fmov d0, x9");                                         // move the boxed float bits into the FP return register
    emitter.instruction("bl __rt_ftoa");                                        // convert the boxed float to a printable string
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("b __rt_mixed_write_stdout_done");                      // restore x30 and return after printing the boxed float

    emitter.label("__rt_mixed_write_stdout_done");
    emitter.instruction("ldr x30, [sp]");                                       // restore the caller return address after any nested helper calls
    emitter.instruction("add sp, sp, #16");                                     // deallocate the mixed-write frame
    emitter.instruction("ret");                                                 // return to the caller
}

fn emit_mixed_write_stdout_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_write_stdout ---");
    emitter.label_global("__rt_mixed_write_stdout");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before any nested helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base so nested calls stay 16-byte aligned
    emitter.instruction("test rax, rax");                                       // null mixed pointers print nothing
    emitter.instruction("je __rt_mixed_write_stdout_done");                     // skip printing when the mixed value is absent
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed runtime payload tag from the mixed cell header
    emitter.instruction("cmp r10, 8");                                          // is the boxed value null?
    emitter.instruction("je __rt_mixed_write_stdout_done");                     // null prints nothing
    emitter.instruction("cmp r10, 3");                                          // is the boxed value a bool?
    emitter.instruction("je __rt_mixed_write_stdout_bool");                     // booleans need PHP echo semantics
    emitter.instruction("cmp r10, 0");                                          // is the boxed value an integer?
    emitter.instruction("je __rt_mixed_write_stdout_int");                      // integers print through the shared integer-to-string helper
    emitter.instruction("cmp r10, 2");                                          // is the boxed value a float?
    emitter.instruction("je __rt_mixed_write_stdout_float");                    // floats print through the shared float-to-string helper
    emitter.instruction("cmp r10, 1");                                          // is the boxed value a string?
    emitter.instruction("jne __rt_mixed_write_stdout_done");                    // non-scalar boxed payloads print nothing for echo
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // load the boxed string pointer into the Linux write buffer register
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // load the boxed string length into the Linux write length register
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the boxed string payload directly to stdout
    emitter.instruction("jmp __rt_mixed_write_stdout_done");                    // skip the scalar conversion helpers after the direct string write

    emitter.label("__rt_mixed_write_stdout_bool");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed bool payload into the integer conversion register
    emitter.instruction("test rax, rax");                                       // false prints an empty string under PHP echo semantics
    emitter.instruction("je __rt_mixed_write_stdout_done");                     // skip output entirely when the boxed bool is false
    emitter.instruction("call __rt_itoa");                                      // true prints as integer 1 via the shared integer-to-string helper
    emitter.instruction("mov rsi, rax");                                        // move the formatted string pointer into the Linux write buffer register
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the converted bool string payload to stdout
    emitter.instruction("jmp __rt_mixed_write_stdout_done");                    // return after printing the boxed bool payload

    emitter.label("__rt_mixed_write_stdout_int");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed integer payload into the shared integer conversion register
    emitter.instruction("call __rt_itoa");                                      // convert the boxed integer to its decimal string representation
    emitter.instruction("mov rsi, rax");                                        // move the formatted string pointer into the Linux write buffer register
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the converted integer string payload to stdout
    emitter.instruction("jmp __rt_mixed_write_stdout_done");                    // return after printing the boxed integer payload

    emitter.label("__rt_mixed_write_stdout_float");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the boxed float bits into a scratch register before the float conversion call
    emitter.instruction("movq xmm0, r10");                                      // move the boxed float bits into the standard x86_64 float argument register
    emitter.instruction("call __rt_ftoa");                                      // convert the boxed float to a printable string
    emitter.instruction("mov rsi, rax");                                        // move the formatted string pointer into the Linux write buffer register
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the converted float string payload to stdout

    emitter.label("__rt_mixed_write_stdout_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning from the mixed echo helper
    emitter.instruction("ret");                                                 // return to the caller after the mixed echo path completes
}
