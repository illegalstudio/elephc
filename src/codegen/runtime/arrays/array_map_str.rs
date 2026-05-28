//! Purpose:
//! Emits the `__rt_array_map_str` and `__rt_array_map_str_owned` runtime helpers.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Applies a user callback to each element of a source array, producing a new string array.
///
/// # Input registers (ARM64 calling convention)
/// - `x0`: callback function pointer
/// - `x1`: source array pointer
/// - `x2`: optional callback environment/capture pointer (0 if not used)
///
/// # Element dispatch
/// - Int source arrays (`elem_size=8`): callback receives element in `x0` as integer.
///   When `x2 != 0`, the environment pointer is passed in `x1` as well.
/// - String source arrays (`elem_size=16`): callback receives string pointer in `x0`
///   and length in `x1`. When `x2 != 0`, the environment pointer is passed in `x2`.
///
/// # Callback output
/// - Returns string result in `x1` (pointer) and `x2` (length), which is persisted
///   to the heap via `__rt_str_persist` before being written to the destination array.
///
/// # Output
/// - `x0`: pointer to a newly allocated string array (`elem_size=16`), one entry per
///   source element, in the same order. The caller owns the returned array.
pub fn emit_array_map_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map_str ---");
    emitter.label_global("__rt_array_map_str");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved x19, x20
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved x21 for the optional callback environment
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("mov x21, x2");                                         // x21 = optional callback environment pointer

    // -- read source array metadata --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("ldr x10, [x1, #16]");                                  // x10 = source elem_size (8=int, 16=str)
    emitter.instruction("str x10, [sp, #24]");                                  // save source elem_size to stack

    // -- create new result array with elem_size=16 (string output) --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array
    emitter.instruction("mov x1, #16");                                         // x1 = element size (16 bytes for string)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0
    emitter.instruction("mov x20, x0");                                         // x20 = new array pointer (callee-saved)

    // -- set up loop counter --
    emitter.instruction("mov x0, #0");                                          // x0 = loop index i = 0
    emitter.instruction("str x0, [sp, #0]");                                    // reuse sp+0 for loop index (callback addr in x19)

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_str_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load loop index
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x0, x9");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_map_str_done");                        // if i >= length, loop complete

    // -- load element from source array based on elem_size --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload source elem_size
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("mul x11, x0, x10");                                    // x11 = i * elem_size
    emitter.instruction("add x11, x1, x11");                                    // x11 = &source_data[i]

    emitter.instruction("cmp x10, #16");                                        // is source a string array?
    emitter.instruction("b.eq __rt_array_map_str_load_str");                    // yes — load ptr+len

    // -- int source: pass element in x0 (first int param) --
    emitter.instruction("ldr x0, [x11]");                                       // x0 = int element
    emitter.instruction("cbz x21, __rt_array_map_str_call");                    // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov x1, x21");                                         // pass capture environment as the scalar wrapper's second argument
    emitter.instruction("b __rt_array_map_str_call");                           // proceed to call

    // -- string source: pass element in x0/x1 (first string param = 2 int regs) --
    emitter.label("__rt_array_map_str_load_str");
    emitter.instruction("ldr x0, [x11]");                                       // x0 = string pointer (first half)
    emitter.instruction("ldr x1, [x11, #8]");                                   // x1 = string length (second half)
    emitter.instruction("cbz x21, __rt_array_map_str_call");                    // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov x2, x21");                                         // pass capture environment after the string pointer/length pair

    // -- call callback --
    emitter.label("__rt_array_map_str_call");
    emitter.instruction("blr x19");                                             // call callback → string result in x1=ptr, x2=len

    // -- persist string result to heap --
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=heap_ptr, x2=len

    // -- store string result in new array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index
    emitter.instruction("add x9, x20, #24");                                    // new array data region
    emitter.instruction("lsl x10, x0, #4");                                     // x10 = i * 16 (string stride)
    emitter.instruction("str x1, [x9, x10]");                                   // store string pointer
    emitter.instruction("add x10, x10, #8");                                    // advance to length slot
    emitter.instruction("str x2, [x9, x10]");                                   // store string length

    // -- advance loop --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index
    emitter.instruction("add x0, x0, #1");                                      // i += 1
    emitter.instruction("str x0, [sp, #0]");                                    // save updated index
    emitter.instruction("b __rt_array_map_str_loop");                           // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_map_str_done");
    emitter.instruction("mov x0, x20");                                         // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped string array
}

/// Applies a callback that returns already-owned strings to each source element.
///
/// This helper is used by descriptor callback wrappers. The wrapper detaches the
/// returned string from the boxed Mixed result and releases the Mixed owner before
/// returning, so this runtime helper stores the returned string pair directly
/// instead of persisting it again.
pub fn emit_array_map_str_owned(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map_str_owned_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map_str_owned ---");
    emitter.label_global("__rt_array_map_str_owned");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate string-map-owned loop metadata
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved callback and destination registers
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved callback environment register
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer for the mapping loop
    emitter.instruction("mov x19, x0");                                         // keep callback address across every loop callback
    emitter.instruction("mov x21, x2");                                         // keep descriptor callback environment pointer across the loop

    // -- read source array metadata --
    emitter.instruction("ldr x9, [x1]");                                        // read source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save source length across callback calls
    emitter.instruction("ldr x10, [x1, #16]");                                  // read source element width for callback ABI dispatch
    emitter.instruction("str x10, [sp, #24]");                                  // save source element width across callback calls

    // -- create new result array with string slots --
    emitter.instruction("mov x0, x9");                                          // pass source length as destination capacity
    emitter.instruction("mov x1, #16");                                         // request 16-byte destination slots for owned strings
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array storage
    emitter.instruction("mov x20, x0");                                         // keep destination array pointer across callback calls

    // -- set up loop counter --
    emitter.instruction("mov x0, #0");                                          // initialize logical loop index to zero
    emitter.instruction("str x0, [sp, #0]");                                    // save loop index in the local frame

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_str_owned_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load current logical loop index
    emitter.instruction("ldr x9, [sp, #16]");                                   // load saved source array length
    emitter.instruction("cmp x0, x9");                                          // check whether every source element has been mapped
    emitter.instruction("b.ge __rt_array_map_str_owned_done");                  // exit once the loop index reaches the source length

    // -- load element from source array based on elem_size --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload source element width
    emitter.instruction("add x1, x1, #24");                                     // advance to the source payload region
    emitter.instruction("mul x11, x0, x10");                                    // compute current source element byte offset
    emitter.instruction("add x11, x1, x11");                                    // compute current source element address
    emitter.instruction("cmp x10, #16");                                        // does the source array hold string ptr/len pairs?
    emitter.instruction("b.eq __rt_array_map_str_owned_load_str");              // branch to the string-input callback ABI

    // -- int source: pass element in x0 and env in x1 --
    emitter.instruction("ldr x0, [x11]");                                       // load scalar source element for the callback
    emitter.instruction("mov x1, x21");                                         // pass descriptor callback environment after the scalar argument
    emitter.instruction("b __rt_array_map_str_owned_call");                     // invoke the callback through the shared call site

    // -- string source: pass element in x0/x1 and env in x2 --
    emitter.label("__rt_array_map_str_owned_load_str");
    emitter.instruction("ldr x0, [x11]");                                       // load source string pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load source string length
    emitter.instruction("mov x2, x21");                                         // pass descriptor callback environment after the string pair

    // -- call callback and store its owned string result directly --
    emitter.label("__rt_array_map_str_owned_call");
    emitter.instruction("blr x19");                                             // call callback, returning owned string in x1=ptr, x2=len
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index after callback clobbers caller-saved registers
    emitter.instruction("add x9, x20, #24");                                    // compute destination payload base
    emitter.instruction("lsl x10, x0, #4");                                     // compute destination string slot byte offset
    emitter.instruction("str x1, [x9, x10]");                                   // store owned string pointer in destination slot
    emitter.instruction("add x10, x10, #8");                                    // advance to destination string length word
    emitter.instruction("str x2, [x9, x10]");                                   // store owned string length in destination slot

    // -- advance loop --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index before incrementing
    emitter.instruction("add x0, x0, #1");                                      // advance to the next source element
    emitter.instruction("str x0, [sp, #0]");                                    // save updated loop index
    emitter.instruction("b __rt_array_map_str_owned_loop");                     // continue mapping owned string results

    // -- set length on new array and return --
    emitter.label("__rt_array_map_str_owned_done");
    emitter.instruction("mov x0, x20");                                         // return destination array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length for destination length publication
    emitter.instruction("str x9, [x0]");                                        // publish mapped destination length
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore callee-saved callback environment register
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callback and destination callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release string-map-owned frame storage
    emitter.instruction("ret");                                                 // return mapped string array pointer in x0
}

/// x86_64 Linux implementation of `emit_array_map_str`.
/// Uses the System V AMD64 ABI: callback in `rdi`, source array in `rsi`, env in `rdx`.
/// Result string returned via `rax` (pointer) and `rdx` (length).
/// Destination array is built with `__rt_array_push_str` for dynamic reallocation safety.
fn emit_array_map_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map_str ---");
    emitter.label_global("__rt_array_map_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving string-map spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the callback, source array metadata, and destination array pointer
    emitter.instruction("push r12");                                            // preserve the callback address register because the mapping loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the loop-index register because the mapping loop keeps it live across callback invocations
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for source metadata, destination array pointer, and optional callback environment
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the string-mapping loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the source array pointer so the loop can reload it after callback and persist helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save optional callback environment pointer for captured-closure wrappers
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the indexed-array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load the source element stride so the loop can distinguish scalar and string inputs
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the source element stride across the destination-array allocation call
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the destination capacity to __rt_array_new
    emitter.instruction("mov rsi, 16");                                         // request 16-byte destination slots because array_map_str always returns strings
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array with the same logical capacity as the source array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the destination array pointer for the loop body and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the string-mapping loop at logical index zero

    emitter.label("__rt_array_map_str_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // stop once the loop index reaches the saved source-array length
    emitter.instruction("jge __rt_array_map_str_done");                         // exit the mapping loop when every source element has been transformed into a string
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the source array pointer after the previous callback/persist helper calls
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the source element stride so the loop can decode the current source slot
    emitter.instruction("mov rcx, r13");                                        // copy the logical source index before scaling it by the source element stride
    emitter.instruction("imul rcx, r11");                                       // convert the logical index into the byte offset of the current source slot
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute the address of the current source slot inside the indexed-array payload region
    emitter.instruction("cmp r11, 16");                                         // does the source array already contain string ptr/len pairs?
    emitter.instruction("je __rt_array_map_str_load_str");                      // branch to the string-input path when the current source slot is a 16-byte string pair
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load the scalar source element into the first SysV integer argument register for the callback
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_map_str_call");                          // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass capture environment as the scalar wrapper's second argument
    emitter.instruction("jmp __rt_array_map_str_call");                         // continue into the shared callback invocation path

    emitter.label("__rt_array_map_str_load_str");
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load the source string pointer into the first SysV integer argument register for the callback
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load the source string length into the second SysV integer argument register for the callback
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_map_str_call");                          // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // pass capture environment after the string pointer/length pair

    emitter.label("__rt_array_map_str_call");
    emitter.instruction("call r12");                                            // invoke the user callback and read the produced string result from rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");                                        // move the callback-produced string pointer into the x86_64 array-push string payload register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the destination array pointer into the x86_64 array-push receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist and append the callback-produced string into the destination array, returning the possibly-grown array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the destination array pointer after the string-append helper may have reallocated storage
    emitter.instruction("add r13, 1");                                          // advance the loop index after materializing the mapped destination string slot
    emitter.instruction("jmp __rt_array_map_str_loop");                         // continue mapping until the source array has been fully consumed

    emitter.label("__rt_array_map_str_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("add rsp, 48");                                         // release the string-map local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the caller loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the mapped string array pointer
    emitter.instruction("ret");                                                 // return the mapped destination string array pointer in rax
}

/// x86_64 Linux implementation of `emit_array_map_str_owned`.
///
/// The descriptor wrapper returns an owned string in `rax`/`rdx`; this helper
/// stores that pair directly into the pre-sized destination array and transfers
/// ownership to the array slot.
fn emit_array_map_str_owned_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map_str_owned ---");
    emitter.label_global("__rt_array_map_str_owned");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before reserving owned-string map slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for loop metadata
    emitter.instruction("push r12");                                            // preserve callback address across descriptor callback calls
    emitter.instruction("push r13");                                            // preserve loop index across descriptor callback calls
    emitter.instruction("sub rsp, 48");                                         // reserve source, destination, width, and environment slots
    emitter.instruction("mov r12, rdi");                                        // keep callback address in a callee-saved register
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save source array pointer for every loop iteration
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save descriptor callback environment pointer
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load source array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save source length across destination allocation
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load source element width for ABI dispatch
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save source element width across callback calls
    emitter.instruction("mov rdi, r10");                                        // pass source length as destination capacity
    emitter.instruction("mov rsi, 16");                                         // request 16-byte string slots for destination values
    emitter.instruction("call __rt_array_new");                                 // allocate destination string array storage
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save destination array pointer for direct slot stores
    emitter.instruction("xor r13d, r13d");                                      // initialize loop index to zero

    emitter.label("__rt_array_map_str_owned_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // compare loop index against source length
    emitter.instruction("jge __rt_array_map_str_owned_done");                   // exit once every source element has been mapped
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload source array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload source element width
    emitter.instruction("mov rcx, r13");                                        // copy loop index before byte-offset scaling
    emitter.instruction("imul rcx, r11");                                       // compute current source element byte offset
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute current source element address
    emitter.instruction("cmp r11, 16");                                         // does the source slot hold a string pair?
    emitter.instruction("je __rt_array_map_str_owned_load_str");                // branch to string-input callback ABI
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load scalar source element into first callback argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass descriptor environment after the scalar argument
    emitter.instruction("jmp __rt_array_map_str_owned_call");                   // invoke the descriptor callback wrapper

    emitter.label("__rt_array_map_str_owned_load_str");
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load source string pointer for callback argument
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load source string length for callback argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // pass descriptor environment after the string pair

    emitter.label("__rt_array_map_str_owned_call");
    emitter.instruction("call r12");                                            // invoke descriptor callback and receive owned string in rax/rdx
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload destination array pointer after callback clobbers caller-saved regs
    emitter.instruction("mov rcx, r13");                                        // copy loop index before destination string-slot scaling
    emitter.instruction("shl rcx, 4");                                          // compute 16-byte destination string slot offset
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute destination string slot address
    emitter.instruction("mov QWORD PTR [rcx], rax");                            // transfer owned string pointer into destination slot
    emitter.instruction("mov QWORD PTR [rcx + 8], rdx");                        // transfer owned string length into destination slot
    emitter.instruction("add r13, 1");                                          // advance loop index after storing the mapped string
    emitter.instruction("jmp __rt_array_map_str_owned_loop");                   // continue mapping owned string results

    emitter.label("__rt_array_map_str_owned_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload destination array pointer for return
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload source length for destination length publication
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish destination array length
    emitter.instruction("add rsp, 48");                                         // release owned-string map local slots
    emitter.instruction("pop r13");                                             // restore loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore callback-address callee-saved register
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return mapped string array pointer in rax
}
