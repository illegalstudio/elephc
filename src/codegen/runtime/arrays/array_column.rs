use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_column: extract a column from an array of associative arrays.
/// Input: x0=outer array (Array of AssocArray), x1=column key ptr, x2=column key len
/// Output: x0=new array containing the column values
pub fn emit_array_column(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_column_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_column ---");
    emitter.label_global("__rt_array_column");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save outer array pointer
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save column key ptr/len

    // -- load outer array length --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = outer array length
    emitter.instruction("str x9, [sp, #24]");                                   // save outer length

    // -- create result array (string values, elem_size=16) --
    emitter.instruction("mov x0, x9");                                          // capacity = outer length
    emitter.instruction("mov x1, #8");                                          // element size = 8 (values)
    emitter.instruction("bl __rt_array_new");                                   // create result array
    emitter.instruction("str x0, [sp, #32]");                                   // save result array pointer

    // -- iterate outer array --
    emitter.instruction("str xzr, [sp, #40]");                                  // loop index = 0

    emitter.label("__rt_ac_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load outer length
    emitter.instruction("cmp x9, x10");                                         // compare index with length
    emitter.instruction("b.ge __rt_ac_done");                                   // if done, exit loop

    // -- load inner assoc array pointer at index --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload outer array
    emitter.instruction("add x0, x0, #24");                                     // skip header
    emitter.instruction("ldr x0, [x0, x9, lsl #3]");                            // load inner hash table pointer at index

    // -- look up column key in inner hash table --
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload column key ptr/len
    emitter.instruction("bl __rt_hash_get");                                    // lookup → x0=found, x1=val_lo, x2=val_hi

    // -- if found, push value to result array --
    emitter.instruction("cbz x0, __rt_ac_skip");                                // skip if key not found

    // -- push value to result array --
    emitter.instruction("mov x3, x1");                                          // save val_lo
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload result array
    emitter.instruction("mov x1, x3");                                          // value
    emitter.instruction("bl __rt_array_push_int");                              // push value to result

    emitter.label("__rt_ac_skip");
    // -- increment index --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload index
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction("str x9, [sp, #40]");                                   // save updated index
    emitter.instruction("b __rt_ac_loop");                                      // continue loop

    emitter.label("__rt_ac_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return result array

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_array_column_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_column ---");
    emitter.label_global("__rt_array_column");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-column spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the outer array pointer, key string, and result array
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the outer array pointer, key string, outer length, result array, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the outer indexed-array pointer across helper calls inside the column-extraction loop
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested column key pointer across helper calls inside the column-extraction loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the requested column key length across helper calls inside the column-extraction loop
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the outer indexed-array logical length so the result array can be sized exactly once
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the outer indexed-array logical length for the loop termination check
    emitter.instruction("mov rdi, r10");                                        // pass the outer indexed-array logical length as the result-array capacity argument
    emitter.instruction("mov rsi, 8");                                          // choose 8-byte scalar slots for the extracted indexed-array result values
    emitter.instruction("call __rt_array_new");                                 // allocate the result indexed array before walking the outer array of associative arrays
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the result indexed-array pointer across hash lookups and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the outer indexed-array loop cursor to the first row

    emitter.label("__rt_ac_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current outer indexed-array loop cursor before checking whether extraction is complete
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // compare the current outer indexed-array loop cursor against the saved logical length
    emitter.instruction("jae __rt_ac_done");                                    // stop once every outer row has been examined for the requested column
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the outer indexed-array pointer after prior helper calls clobbered caller-saved registers
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10 * 8 + 24]");             // load the current inner associative-array hash pointer from the outer indexed-array payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested column key pointer for the inner associative-array lookup
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the requested column key length for the inner associative-array lookup
    emitter.instruction("call __rt_hash_get");                                  // look up the requested column key in the current inner associative-array row
    emitter.instruction("test rax, rax");                                       // check whether the current inner associative array contains the requested column key
    emitter.instruction("jz __rt_ac_next");                                     // skip append work when the requested column key is missing from the current row
    emitter.instruction("mov r10, rdi");                                        // preserve the borrowed scalar payload returned by the hash lookup before loading append arguments
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the result indexed-array pointer for the scalar append helper
    emitter.instruction("mov rsi, r10");                                        // move the borrowed scalar payload into the indexed-array append helper value register
    emitter.instruction("call __rt_array_push_int");                            // append the extracted scalar column value to the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the possibly-reallocated result indexed-array pointer after the append helper returns

    emitter.label("__rt_ac_next");
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance the outer indexed-array loop cursor to the next associative-array row
    emitter.instruction("jmp __rt_ac_loop");                                    // continue scanning the outer indexed array for additional requested column values

    emitter.label("__rt_ac_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the extracted scalar column values array in the standard integer result register
    emitter.instruction("add rsp, 48");                                         // release the array-column spill slots before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the extracted indexed array
    emitter.instruction("ret");                                                 // return the extracted scalar column values array to generated code
}
