use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// build_argv: create a PHP $argv array from OS argc/argv.
/// Reads _global_argc and _global_argv, builds a string array.
/// Output: x0 = pointer to array
pub fn emit_build_argv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_build_argv_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: build_argv ---");
    emitter.label_global("__rt_build_argv");

    // -- set up stack frame (48 bytes for locals + saved registers) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer

    // -- load argc from the global variable --
    abi::emit_load_symbol_to_reg(emitter, "x19", "_global_argc", 0);

    // -- load argv pointer from the global variable --
    abi::emit_load_symbol_to_reg(emitter, "x20", "_global_argv", 0);

    // -- save callee-saved registers we're about to use --
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save x19 (argc) and x20 (argv) to stack
    emitter.instruction("str x21, [sp, #16]");                                  // save x21 (will hold array pointer)

    // -- create a new string array with capacity = argc --
    emitter.instruction("mov x0, x19");                                         // arg0: capacity = argc
    emitter.instruction("mov x1, #16");                                         // arg1: elem_size = 16 (ptr + len per string)
    emitter.instruction("bl __rt_array_new");                                   // allocate the array, x0 = array pointer
    emitter.instruction("mov x21, x0");                                         // x21 = array pointer (save in callee-saved reg)

    // -- initialize loop counter i = 0 --
    emitter.instruction("mov x22, #0");                                         // x22 = 0 (loop counter)
    emitter.instruction("str x22, [sp, #24]");                                  // store i on stack (survives function calls)

    // -- loop: for i = 0..argc, convert each C string and push to array --
    emitter.label("__rt_build_argv_loop");
    emitter.instruction("ldr x22, [sp, #24]");                                  // reload i from stack
    emitter.instruction("cmp x22, x19");                                        // compare i with argc
    emitter.instruction("b.ge __rt_build_argv_done");                           // if i >= argc, exit loop

    // -- get pointer to argv[i] (C string) --
    emitter.instruction("ldr x1, [x20, x22, lsl #3]");                          // x1 = argv[i] (load pointer at argv + i*8)

    // -- compute string length by scanning for null terminator --
    emitter.instruction("mov x2, #0");                                          // x2 = 0 (length counter)
    emitter.label("__rt_build_argv_strlen");
    emitter.instruction("ldrb w3, [x1, x2]");                                   // w3 = byte at str[length] (load single byte)
    emitter.instruction("cbz w3, __rt_build_argv_push");                        // if byte == 0 (null terminator), done counting
    emitter.instruction("add x2, x2, #1");                                      // length += 1
    emitter.instruction("b __rt_build_argv_strlen");                            // continue scanning

    // -- push the string (ptr in x1, len in x2) to the array --
    emitter.label("__rt_build_argv_push");
    emitter.instruction("mov x0, x21");                                         // arg0: array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push string element to array
    emitter.instruction("mov x21, x0");                                         // update array pointer after possible realloc

    // -- increment loop counter and continue --
    emitter.instruction("ldr x22, [sp, #24]");                                  // reload i from stack (may have been clobbered)
    emitter.instruction("add x22, x22, #1");                                    // i += 1
    emitter.instruction("str x22, [sp, #24]");                                  // save updated i back to stack
    emitter.instruction("b __rt_build_argv_loop");                              // continue loop

    // -- loop complete, return the array pointer --
    emitter.label("__rt_build_argv_done");
    emitter.instruction("mov x0, x21");                                         // return value: array pointer in x0

    // -- restore callee-saved registers and tear down stack frame --
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore x19 (argc) and x20 (argv)
    emitter.instruction("ldr x21, [sp, #16]");                                  // restore x21
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_build_argv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: build_argv ---");
    emitter.label_global("__rt_build_argv");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving local scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for argc/argv bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for argc, argv, result pointer, and loop index

    abi::emit_load_symbol_to_reg(emitter, "r8", "_global_argc", 0);
    abi::emit_load_symbol_to_reg(emitter, "r9", "_global_argv", 0);
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save argc across the malloc call and later loop iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the OS argv pointer array across the malloc call

    emitter.instruction("mov rdi, r8");                                         // seed malloc's size argument from argc
    emitter.instruction("shl rdi, 4");                                          // reserve 16 bytes per argv entry for ptr+len storage
    emitter.instruction("add rdi, 24");                                         // include the fixed 24-byte array header in the allocation size
    emitter.instruction("call malloc");                                         // allocate the argv array backing storage with libc malloc
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the allocated array pointer for the loop body and final return

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload argc after malloc may have clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax], r8");                             // header[0] = logical argv length
    emitter.instruction("mov QWORD PTR [rax + 8], r8");                         // header[8] = argv capacity
    emitter.instruction("mov QWORD PTR [rax + 16], 16");                        // header[16] = elem_size for string ptr+len pairs
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the argv loop counter to zero

    emitter.label("__rt_build_argv_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current argv element index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 8]");                        // compare the loop index against argc
    emitter.instruction("jae __rt_build_argv_done");                            // stop once every OS argv entry has been materialized

    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the OS argv pointer table base
    emitter.instruction("mov r11, QWORD PTR [r10 + rcx * 8]");                  // load argv[i] as a raw C string pointer
    emitter.instruction("xor rdx, rdx");                                        // reset the byte-count accumulator before scanning for the null terminator

    emitter.label("__rt_build_argv_strlen");
    emitter.instruction("mov al, BYTE PTR [r11 + rdx]");                        // read the next byte from argv[i] while measuring its PHP string length
    emitter.instruction("test al, al");                                         // check whether the current byte is the terminating NUL
    emitter.instruction("je __rt_build_argv_store");                            // stop scanning once the C string terminator is reached
    emitter.instruction("add rdx, 1");                                          // advance the measured argv[i] length by one byte
    emitter.instruction("jmp __rt_build_argv_strlen");                          // continue scanning the current argv[i] C string

    emitter.label("__rt_build_argv_store");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the destination argv array pointer
    emitter.instruction("mov rsi, rcx");                                        // copy the element index into a scratch register for 16-byte slot addressing
    emitter.instruction("shl rsi, 4");                                          // convert the argv element index into a byte offset
    emitter.instruction("add r10, rsi");                                        // advance to the selected argv element slot
    emitter.instruction("add r10, 24");                                         // skip the array header to reach element storage
    emitter.instruction("mov QWORD PTR [r10], r11");                            // store argv[i]'s raw pointer as the PHP string payload pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // store argv[i]'s measured length beside the pointer

    emitter.instruction("add rcx, 1");                                          // increment the argv loop counter
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated loop counter for the next iteration
    emitter.instruction("jmp __rt_build_argv_loop");                            // continue materializing the remaining argv entries

    emitter.label("__rt_build_argv_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the argv array pointer in the integer result register
    emitter.instruction("add rsp, 32");                                         // release the local argc/argv scratch slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the materialized argv array header pointer
}

#[cfg(test)]
mod tests {
    use crate::codegen::platform::{Arch, Platform, Target};

    use super::*;

    #[test]
    fn test_emit_build_argv_linux_x86_64_uses_malloc_backing() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_build_argv(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_build_argv:\n"));
        assert!(asm.contains("call malloc\n"));
        assert!(asm.contains("mov QWORD PTR [rax + 16], 16\n"));
        assert!(asm.contains("mov QWORD PTR [r10 + 8], rdx\n"));
    }
}
