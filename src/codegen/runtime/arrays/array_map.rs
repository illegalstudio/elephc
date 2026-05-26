//! Purpose:
//! Emits the `__rt_array_map`, `__rt_array_new` runtime helper assembly for array map.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_map` runtime helper for ARM64 (macOS/Linux).
///
/// Iterates over every element in the source array, invokes the callback with the
/// current element (and an optional capture environment pointer), and stores each
/// transformed result into a newly allocated destination array. The callback ABI
/// differs for scalar values (single register) versus string elements (ptr/len pair).
///
/// # Input registers
/// - `x0`: callback function address
/// - `x1`: source array pointer
/// - `x2`: optional callback environment pointer (capture closure)
///
/// # Output registers
/// - `x0`: pointer to the new mapped array (same length as source)
///
/// # Register usage
/// - `x19`: callee-saved callback address
/// - `x20`: loop index (callee-saved)
/// - `x9`–`x11`: scratch temporaries
pub fn emit_array_map(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map ---");
    emitter.label_global("__rt_array_map");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack space for scalar mapping metadata
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved x19, x20
    emitter.instruction("str x2, [sp, #0]");                                    // save optional callback environment pointer to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- read source array length and create new array --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("ldr x10, [x1, #16]");                                  // load source element width for scalar/string callback ABI dispatch
    emitter.instruction("str x10, [sp, #32]");                                  // save source element width across callback calls
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array
    emitter.instruction("mov x1, #8");                                          // x1 = element size (8 bytes for int)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0=new array ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_map_done");                            // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload source element width for callback argument loading
    emitter.instruction("cmp x10, #16");                                        // does the source array contain string ptr/len slots?
    emitter.instruction("b.eq __rt_array_map_load_str");                        // use the string callback ABI for 16-byte source elements
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_map_call");                         // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov x1, x9");                                          // pass capture environment as the wrapper's second argument
    emitter.instruction("b __rt_array_map_call");                               // call through the shared callback branch
    emitter.label("__rt_array_map_load_str");
    emitter.instruction("lsl x11, x20, #4");                                    // compute source string byte offset from the logical index
    emitter.instruction("add x11, x1, x11");                                    // compute address of the current source string slot
    emitter.instruction("ldr x0, [x11]");                                       // load source string pointer for callback
    emitter.instruction("ldr x1, [x11, #8]");                                   // load source string length for callback
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_map_call");                         // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov x2, x9");                                          // pass capture environment after the string ptr/len pair

    // -- call callback with element as argument --
    emitter.label("__rt_array_map_call");
    emitter.instruction("blr x19");                                             // call callback(element) → result in x0

    // -- store result in new array --
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload new array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip header to data region
    emitter.instruction("str x0, [x2, x20, lsl #3]");                           // new_array[i] = callback result

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_map_loop");                               // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_map_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped array
}

/// Emits the `__rt_array_map` runtime helper for x86_64 Linux (System V ABI).
///
/// Same behavior as the ARM64 variant but uses the x86_64 System V calling convention.
/// The callback receives the element in `rdi` (or `rdi`/`rsi` for strings) and an
/// optional capture environment in `rdx`. The transformed result is returned in `rax`.
///
/// # Input registers
/// - `rdi`: callback function address
/// - `rsi`: source array pointer
/// - `rdx`: optional callback environment pointer
///
/// # Output registers
/// - `rax`: pointer to the new mapped array
///
/// # Register usage
/// - `r12`: callee-saved callback address
/// - `r13`: loop index (callee-saved)
/// - `r10`–`r11`: scratch temporaries
fn emit_array_map_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map ---");
    emitter.label_global("__rt_array_map");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-map spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the callback, source array, and destination array slots
    emitter.instruction("push r12");                                            // preserve the callback scratch register because the runtime uses it across every callback invocation
    emitter.instruction("push r13");                                            // preserve the loop-index scratch register because the runtime keeps it live across callback calls
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for source metadata, destination array pointer, element width, and optional callback environment
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the mapping loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save optional callback environment pointer for captured-closure wrappers
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load source element width for scalar/string callback ABI dispatch
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save source element width across callback calls
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the destination capacity to __rt_array_new
    emitter.instruction("mov rsi, 8");                                          // request 8-byte element slots for the integer-returning array_map runtime
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array with the same logical capacity as the source array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the destination array pointer for the loop body and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the mapping loop at logical index zero

    emitter.label("__rt_array_map_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // stop once the loop index reaches the saved source array length
    emitter.instruction("jge __rt_array_map_done");                             // exit the mapping loop when every source element has been transformed
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the source array pointer after the previous callback invocation
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload source element width for callback argument loading
    emitter.instruction("cmp r11, 16");                                         // does the source array contain string ptr/len slots?
    emitter.instruction("je __rt_array_map_load_str");                          // use the string callback ABI for 16-byte source elements
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current source element into the first SysV integer argument register
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_map_call");                              // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass capture environment as the wrapper's second argument
    emitter.instruction("jmp __rt_array_map_call");                             // call through the shared callback branch
    emitter.label("__rt_array_map_load_str");
    emitter.instruction("mov rcx, r13");                                        // copy logical source index before scaling to a string slot offset
    emitter.instruction("shl rcx, 4");                                          // convert source index into a 16-byte string-slot offset
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute address of the current source string slot
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load source string pointer for callback
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load source string length for callback
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_map_call");                              // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // pass capture environment after the string ptr/len pair
    emitter.label("__rt_array_map_call");
    emitter.instruction("call r12");                                            // invoke the user callback with the current element and read the transformed value from rax
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the destination array pointer after the callback clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [r10 + r13 * 8 + 24], rax");             // store the transformed value into the matching destination-array element slot
    emitter.instruction("add r13, 1");                                          // advance the loop index after storing the transformed destination element
    emitter.instruction("jmp __rt_array_map_loop");                             // continue mapping until the source array has been fully consumed

    emitter.label("__rt_array_map_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the saved source length so the destination logical length matches the mapped input size
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the mapped destination length in the destination array header
    emitter.instruction("add rsp, 48");                                         // release the local source/destination bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the caller's loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller's callback scratch callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the mapped array pointer
    emitter.instruction("ret");                                                 // return the mapped destination array pointer in rax
}
