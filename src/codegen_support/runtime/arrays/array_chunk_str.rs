//! Purpose:
//! Emits the `__rt_array_chunk_str` runtime helper assembly for chunking an indexed string array.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. `__rt_array_chunk*`
//!   copy 8 bytes per element, so they cannot carry a string pair (issue #675).
//! - The outer array holds 8-byte inner-array pointers, built exactly like
//!   `__rt_array_chunk_refcounted`: each fresh chunk is appended with `__rt_array_push_int`,
//!   which transfers the chunk's only reference. The caller stamps the outer value type.
//! - Each chunk's strings are DUPLICATED through `__rt_array_push_str`, which also stamps the
//!   chunk's 16-byte string shape on its first append.
//! - A chunk's capacity is `min(size, remaining)`, so a `$length` far larger than the array does
//!   not ask the allocator for a huge buffer. The lowering guarantees `size >= 1`.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_array_chunk_str` runtime helper.
///
/// Splits an indexed string array into consecutive chunks of at most `size` strings, returning an
/// indexed array of indexed string arrays. The source is never modified.
///
/// ## ABI
/// - **ARM64**: `x0` = source array, `x1` = chunk size (at least 1). Result returned in `x0`.
/// - **x86_64 Linux**: `rdi`, `rsi` with the same meaning. Result in `rax`.
pub fn emit_array_chunk_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_chunk_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_chunk_str ---");
    emitter.label_global("__rt_array_chunk_str");

    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the chunk size
    emitter.instruction("ldr x2, [x0]");                                        // load the source length
    emitter.instruction("sub x3, x1, #1");                                      // compute size - 1 for the ceiling division
    emitter.instruction("add x2, x2, x3");                                      // bias the length for the ceiling division
    emitter.instruction("udiv x0, x2, x1");                                     // outer capacity = ceil(length / size)
    emitter.instruction("mov x1, #8");                                          // the outer array stores 8-byte chunk pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate the outer array
    emitter.instruction("str x0, [sp, #16]");                                   // save outer array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // start the source cursor at 0

    emitter.label("__rt_array_chunk_str_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load the source length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the source cursor
    emitter.instruction("cmp x4, x3");                                          // has every source string been assigned to a chunk?
    emitter.instruction("b.ge __rt_array_chunk_str_done");                      // yes: return the outer array
    emitter.instruction("sub x5, x3, x4");                                      // strings left in the source
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload the chunk size
    emitter.instruction("cmp x5, x6");                                          // is the remainder smaller than a full chunk?
    emitter.instruction("csel x0, x5, x6, lt");                                 // chunk capacity = min(size, remaining)
    emitter.instruction("mov x1, #16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate the current chunk
    emitter.instruction("str x0, [sp, #32]");                                   // save the current chunk pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // start the chunk cursor at 0

    emitter.label("__rt_array_chunk_str_inner");
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload the chunk cursor
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload the chunk size
    emitter.instruction("cmp x5, x6");                                          // is the current chunk full?
    emitter.instruction("b.ge __rt_array_chunk_str_push");                      // yes: append it to the outer array
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload the source length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the source cursor
    emitter.instruction("cmp x4, x3");                                          // is the source exhausted?
    emitter.instruction("b.ge __rt_array_chunk_str_push");                      // yes: append the partial chunk
    emitter.instruction("add x7, x0, #24");                                     // compute source data base past the 24-byte header
    emitter.instruction("add x7, x7, x4, lsl #4");                              // address the source {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x7]");                                    // load the borrowed source pointer and length
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the current chunk pointer
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the pair
    emitter.instruction("str x0, [sp, #32]");                                   // persist the chunk pointer after possible growth
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the source cursor across the append call
    emitter.instruction("add x4, x4, #1");                                      // advance the source cursor
    emitter.instruction("str x4, [sp, #24]");                                   // spill the updated source cursor
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload the chunk cursor across the append call
    emitter.instruction("add x5, x5, #1");                                      // advance the chunk cursor
    emitter.instruction("str x5, [sp, #40]");                                   // spill the updated chunk cursor
    emitter.instruction("b __rt_array_chunk_str_inner");                        // continue filling the current chunk

    emitter.label("__rt_array_chunk_str_push");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload outer array pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the finished chunk pointer
    emitter.instruction("bl __rt_array_push_int");                              // transfer the chunk's reference into the outer array
    emitter.instruction("str x0, [sp, #16]");                                   // persist the outer pointer after possible growth
    emitter.instruction("b __rt_array_chunk_str_outer");                        // start the next chunk

    emitter.label("__rt_array_chunk_str_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the outer array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return the chunked array
}

/// Emits the x86_64 Linux variant of `__rt_array_chunk_str`.
///
/// Same semantics as the ARM64 variant under the System V AMD64 ABI: `rdi`, `rsi` in, `rax` out.
fn emit_array_chunk_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_chunk_str ---");
    emitter.label_global("__rt_array_chunk_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the chunk size
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the source length
    emitter.instruction("lea rax, [rax + rsi - 1]");                            // bias the length for the ceiling division
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half
    emitter.instruction("div rsi");                                             // outer capacity = ceil(length / size)
    emitter.instruction("mov rdi, rax");                                        // pass the outer capacity
    emitter.instruction("mov rsi, 8");                                          // the outer array stores 8-byte chunk pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the outer array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the outer array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the source cursor at 0

    emitter.label("__rt_array_chunk_str_outer_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the source length
    emitter.instruction("sub rax, QWORD PTR [rbp - 32]");                       // strings left in the source
    emitter.instruction("jle __rt_array_chunk_str_done_x86");                   // none left: return the outer array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // start from the chunk size
    emitter.instruction("cmp rax, rdi");                                        // is the remainder smaller than a full chunk?
    emitter.instruction("cmovl rdi, rax");                                      // chunk capacity = min(size, remaining)
    emitter.instruction("mov rsi, 16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the current chunk
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the current chunk pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // start the chunk cursor at 0

    emitter.label("__rt_array_chunk_str_inner_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the chunk cursor
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // is the current chunk full?
    emitter.instruction("jge __rt_array_chunk_str_push_x86");                   // yes: append it to the outer array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source cursor
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // is the source exhausted?
    emitter.instruction("jge __rt_array_chunk_str_push_x86");                   // yes: append the partial chunk
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute source data base past the 24-byte header
    emitter.instruction("shl rcx, 4");                                          // scale the cursor by the 16-byte string slot
    emitter.instruction("add r10, rcx");                                        // address the source {pointer, length} pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the borrowed source pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the borrowed source length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the current chunk pointer
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the pair
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the chunk pointer after possible growth
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance the source cursor
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance the chunk cursor
    emitter.instruction("jmp __rt_array_chunk_str_inner_x86");                  // continue filling the current chunk

    emitter.label("__rt_array_chunk_str_push_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the outer array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the finished chunk pointer
    emitter.instruction("call __rt_array_push_int");                            // transfer the chunk's reference into the outer array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the outer pointer after possible growth
    emitter.instruction("jmp __rt_array_chunk_str_outer_x86");                  // start the next chunk

    emitter.label("__rt_array_chunk_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the outer array pointer
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the chunked array
}
