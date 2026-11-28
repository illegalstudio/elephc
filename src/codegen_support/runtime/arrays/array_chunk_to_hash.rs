//! Purpose:
//! Emits the `__rt_array_chunk_to_hash` runtime helper backing `array_chunk($a, $n, true)`.
//! Splits an indexed array into an outer indexed array of owned hashes, each keeping the source
//! integer keys of its own window instead of renumbering them from zero.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Each chunk is built by `__rt_array_slice_to_hash`, so the window arithmetic, the string
//!   persistence and the heap retains are shared with `array_slice($a, $o, $l, true)` instead of
//!   being reimplemented. The final chunk is short whenever the source length is not a multiple of
//!   the requested size, which the slice helper's clamp already handles.
//! - The outer array holds pointer-sized hash payloads; its `value_type` is stamped by the
//!   backend after the helper returns, exactly like the scalar and refcounted chunk helpers.
//! - The chunk count is `ceil(length / size)`, computed the same way as `__rt_array_chunk`, so a
//!   chunk size of zero is rejected before the call rather than divided by here.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_chunk_to_hash: split an indexed array into key-preserving hash chunks.
/// Input:  x0 = source indexed array pointer, x1 = chunk size (must be >= 1)
/// Output: x0 = outer indexed array whose elements are owned hash pointers
///
/// Backs `array_chunk($array, $length, preserve_keys: true)`.
pub fn emit_array_chunk_to_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_chunk_to_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_chunk_to_hash ---");
    emitter.label_global("__rt_array_chunk_to_hash");
    emitter.instruction("sub sp, sp, #64");                                     // allocate the chunking stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source indexed array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested chunk size
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "x2", "x0"); // load the source indexed-array logical length
    emitter.instruction("sub x3, x1, #1");                                      // bias the numerator by chunk_size - 1
    emitter.instruction("add x2, x2, x3");                                      // length + chunk_size - 1 for ceiling division
    emitter.instruction("udiv x2, x2, x1");                                     // number of chunks = ceil(length / chunk_size)
    emitter.instruction("mov x0, x2");                                          // outer array capacity = number of chunks
    emitter.instruction("mov x1, #8");                                          // outer slots hold pointer-sized hash payloads
    emitter.instruction("bl __rt_array_new");                                   // allocate the outer indexed array, x0 = outer
    emitter.instruction("str x0, [sp, #16]");                                   // save the outer indexed array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // window cursor i = 0
    emitter.label("__rt_array_chunk_to_hash_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source indexed array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload the source logical length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the window cursor
    emitter.instruction("cmp x4, x3");                                          // has every source element been assigned to a chunk?
    emitter.instruction("b.ge __rt_array_chunk_to_hash_done");                  // finish once the source is exhausted
    emitter.instruction("mov x1, x4");                                          // slice offset = current window cursor
    emitter.instruction("ldr x2, [sp, #8]");                                    // slice length = requested chunk size
    emitter.instruction("mov x3, #1");                                          // the chunk length is always explicitly present
    emitter.instruction("bl __rt_array_slice_to_hash");                         // build this chunk as a key-preserving hash, x0 = chunk
    emitter.instruction("mov x1, x0");                                          // the chunk pointer is the value appended to the outer array
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the outer indexed array pointer
    emitter.instruction("bl __rt_array_push_int");                              // append the finished chunk to the outer array
    emitter.instruction("str x0, [sp, #16]");                                   // publish the possibly-grown outer array pointer
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the window cursor
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload the requested chunk size
    emitter.instruction("add x4, x4, x5");                                      // advance the cursor to the next window
    emitter.instruction("str x4, [sp, #24]");                                   // save the advanced cursor
    emitter.instruction("b __rt_array_chunk_to_hash_loop");                     // continue with the next chunk
    emitter.label("__rt_array_chunk_to_hash_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = outer indexed array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the outer array in x0
}

/// x86_64 Linux implementation of `__rt_array_chunk_to_hash`.
/// Input:  rdi = source indexed array pointer, rsi = chunk size (must be >= 1)
/// Output: rax = outer indexed array whose elements are owned hash pointers
fn emit_array_chunk_to_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_chunk_to_hash ---");
    emitter.label_global("__rt_array_chunk_to_hash");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for the chunking loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source indexed array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested chunk size
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "rax", "rdi"); // load the source indexed-array logical length
    emitter.instruction("mov rcx, rsi");                                        // copy the chunk size before biasing the numerator
    emitter.instruction("sub rcx, 1");                                          // bias the numerator by chunk_size - 1
    emitter.instruction("add rax, rcx");                                        // length + chunk_size - 1 for ceiling division
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing
    emitter.instruction("div rsi");                                             // number of chunks = ceil(length / chunk_size)
    emitter.instruction("mov rdi, rax");                                        // outer array capacity = number of chunks
    emitter.instruction("mov rsi, 8");                                          // outer slots hold pointer-sized hash payloads
    emitter.instruction("call __rt_array_new");                                 // allocate the outer indexed array, rax = outer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the outer indexed array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // window cursor i = 0
    emitter.label("__rt_array_chunk_to_hash_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the window cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed array pointer
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has every source element been assigned to a chunk?
    emitter.instruction("jge __rt_array_chunk_to_hash_done");                   // finish once the source is exhausted
    emitter.instruction("mov rdi, r10");                                        // slice receiver = the source indexed array
    emitter.instruction("mov rsi, rcx");                                        // slice offset = current window cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // slice length = requested chunk size
    emitter.instruction("mov rcx, 1");                                          // the chunk length is always explicitly present
    emitter.instruction("call __rt_array_slice_to_hash");                       // build this chunk as a key-preserving hash, rax = chunk
    emitter.instruction("mov rsi, rax");                                        // the chunk pointer is the value appended to the outer array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the outer indexed array pointer
    emitter.instruction("call __rt_array_push_int");                            // append the finished chunk to the outer array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // publish the possibly-grown outer array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the window cursor
    emitter.instruction("add rcx, QWORD PTR [rbp - 16]");                       // advance the cursor to the next window
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the advanced cursor
    emitter.instruction("jmp __rt_array_chunk_to_hash_loop");                   // continue with the next chunk
    emitter.label("__rt_array_chunk_to_hash_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = outer indexed array pointer
    emitter.instruction("add rsp, 32");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the outer array in rax
}
