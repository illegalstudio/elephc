//! Purpose:
//! Emits the `__rt_array_flip_string`, `__rt_hash_new` runtime helper assembly for array flip string.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_flip_string: swap keys and values for an indexed string array.
/// Creates a hash table where the normalized string value becomes the key and the index becomes the value.
/// Input:  x0=array_ptr (indexed string array)
/// Output: x0=new hash table
pub fn emit_array_flip_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_flip_string_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_flip_string ---");
    emitter.label_global("__rt_array_flip_string");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = hash table pointer (result)
    //   [sp, #16] = loop index i
    //   [sp, #24] = saved x29
    //   [sp, #32] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for source, result, loop index, and frame link
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- create hash table with capacity = array length * 2 (load factor) --
    emitter.instruction("ldr x0, [x0]");                                        // load source array length
    emitter.instruction("lsl x0, x0, #1");                                      // double the capacity to keep hash load low
    emitter.instruction("mov x9, #16");                                         // materialize the minimum hash capacity
    emitter.instruction("cmp x0, x9");                                          // compare requested capacity with the runtime minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp small hashes to the minimum capacity
    emitter.instruction("mov x1, #0");                                          // result hash values are integer source indexes
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash table
    emitter.instruction("str x0, [sp, #8]");                                    // save destination hash table pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize source index i = 0

    emitter.label("__rt_array_flip_string_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load source array length
    emitter.instruction("ldr x4, [sp, #16]");                                   // load current source index
    emitter.instruction("cmp x4, x3");                                          // compare current source index against source length
    emitter.instruction("b.ge __rt_array_flip_string_done");                    // finish once every source element has been flipped

    // -- load array[i] as the candidate string key and normalize PHP numeric-string keys --
    emitter.instruction("add x5, x0, #24");                                     // point at the source string payload region
    emitter.instruction("lsl x6, x4, #4");                                      // convert string element index into a 16-byte slot offset
    emitter.instruction("add x5, x5, x6");                                      // advance to the selected source string slot
    emitter.instruction("ldr x1, [x5]");                                        // load source string pointer as candidate key low word
    emitter.instruction("ldr x2, [x5, #8]");                                    // load source string length as candidate key high word
    emitter.instruction("bl __rt_hash_normalize_key");                          // convert integer-form numeric strings into integer keys

    // -- call hash_set: key=value, value=index --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload destination hash table pointer
    emitter.instruction("ldr x3, [sp, #16]");                                   // use the source index as the flipped hash value
    emitter.instruction("mov x4, #0");                                          // integer values do not use the high payload word
    emitter.instruction("mov x5, #0");                                          // runtime tag 0 = integer source index value
    emitter.instruction("bl __rt_hash_set");                                    // insert or update the flipped key/value pair
    emitter.instruction("str x0, [sp, #8]");                                    // save possibly-grown destination hash table pointer

    // -- advance loop --
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload source index after helper calls
    emitter.instruction("add x4, x4, #1");                                      // advance to the next source element
    emitter.instruction("str x4, [sp, #16]");                                   // persist updated source index
    emitter.instruction("b __rt_array_flip_string_loop");                       // continue flipping source string values

    emitter.label("__rt_array_flip_string_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // publish destination hash pointer as the result
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release stack frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash table
}

/// x86_64 Linux variant of `emit_array_flip_string`.
/// Uses the System V AMD64 ABI: source array pointer in `rdi`, result returned in `rax`.
/// Allocates a hash table with doubled capacity for low load factor, then iterates the
/// source indexed string array, normalizing keys and inserting flipped key/value pairs.
fn emit_array_flip_string_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_flip_string ---");
    emitter.label_global("__rt_array_flip_string");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-flip spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for source, result, and loop index
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the string array-flip loop
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across nested helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the source indexed-array logical length
    emitter.instruction("shl rdi, 1");                                          // double the source length for destination hash headroom
    emitter.instruction("cmp rdi, 16");                                         // compare requested capacity with the runtime minimum
    emitter.instruction("jge __rt_array_flip_string_capacity_x86");             // keep requested capacity when it already meets the minimum
    emitter.instruction("mov rdi, 16");                                         // fall back to the minimum destination hash capacity
    emitter.label("__rt_array_flip_string_capacity_x86");
    emitter.instruction("mov rsi, 0");                                          // result hash values are integer source indexes
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination hash pointer across insertions
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize source index i = 0

    emitter.label("__rt_array_flip_string_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload current source index
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload source indexed-array pointer
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare current source index against source length
    emitter.instruction("jge __rt_array_flip_string_done_x86");                 // finish once every source element has been flipped
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the source string payload base address
    emitter.instruction("mov rax, rcx");                                        // copy source index before scaling to a 16-byte string slot
    emitter.instruction("shl rax, 4");                                          // convert source index into a string slot byte offset
    emitter.instruction("add r11, rax");                                        // advance to the selected source string slot
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load source string pointer as candidate key low word
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load source string length as candidate key high word
    emitter.instruction("call __rt_hash_normalize_key");                        // convert integer-form numeric strings into integer keys
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload destination hash table pointer
    emitter.instruction("mov rsi, rax");                                        // place normalized key low word in the hash-set key register
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload source index after normalization
    emitter.instruction("mov rcx, r10");                                        // use the source index as the flipped hash value
    emitter.instruction("xor r8d, r8d");                                        // integer values do not use the high payload word
    emitter.instruction("mov r9, 0");                                           // runtime tag 0 = integer source index value
    emitter.instruction("call __rt_hash_set");                                  // insert or update the flipped key/value pair
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save possibly-grown destination hash table pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload source index after insertion
    emitter.instruction("add r10, 1");                                          // advance to the next source element
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // persist updated source index
    emitter.instruction("jmp __rt_array_flip_string_loop_x86");                 // continue flipping source string values

    emitter.label("__rt_array_flip_string_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // publish destination hash pointer as the result
    emitter.instruction("add rsp, 32");                                         // release stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = destination hash table
}
