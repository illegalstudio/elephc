//! Purpose:
//! Emits growing raw-DEFLATE decoding for both supported architectures.
//!
//! Called from:
//! - The gzinflate EIR builtin lowering in the parent compression module.
//!
//! Key details:
//! - Output storage doubles without restarting zlib or dropping decoded bytes.
//! - Replaced buffers and failed results are released before returning.

use super::*;

/// Emits AArch64 `gzinflate()` inline raw-inflate calls.
pub(super) fn lower_gzinflate_aarch64(
    ctx: &mut FunctionContext<'_>,
    zero: &str,
    zeroed: &str,
    fail: &str,
    done: &str,
) {
    let again = format!("{}_inflate_again", zero);
    let grow = format!("{}_inflate_grow", zero);
    let finish = format!("{}_inflate_finish", zero);
    // -- save the source and allocate initial output storage --
    ctx.emitter.instruction("sub sp, sp, #176");                                // reserve z_stream storage plus scratch slots
    ctx.emitter.instruction("str x1, [sp, #112]");                              // save the source pointer
    ctx.emitter.instruction("str x2, [sp, #120]");                              // save the source length
    ctx.emitter.instruction("lsl x9, x2, #8");                                  // budget 256x the compressed byte length
    ctx.emitter.instruction("mov x10, #65536");                                 // materialize the minimum inflate buffer size
    ctx.emitter.instruction("cmp x9, x10");                                     // compare the scaled budget against the minimum
    ctx.emitter.instruction("csel x9, x9, x10, gt");                            // choose the larger output buffer size
    ctx.emitter.instruction("str x9, [sp, #144]");                              // save the output capacity
    ctx.emitter.instruction("mov x0, x9");                                      // pass the output capacity to the heap allocator
    ctx.emitter.instruction("bl __rt_heap_alloc");                              // allocate the decompressed-data buffer
    ctx.emitter.instruction("mov x9, #1");                                      // heap kind 1 = owned string
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp the output buffer as a heap string
    ctx.emitter.instruction("str x0, [sp, #128]");                              // save the destination buffer pointer

    // -- zero z_stream before initializing its allocator callbacks --
    ctx.emitter.instruction("mov x9, #0");                                      // initialize the z_stream clear index
    ctx.emitter.label(zero);
    ctx.emitter.instruction("cmp x9, #112");                                    // check whether every z_stream byte is cleared
    ctx.emitter.instruction(&format!("b.ge {}", zeroed));                       // continue after the z_stream has been zeroed
    ctx.emitter.instruction("strb wzr, [sp, x9]");                              // clear one z_stream byte
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance the z_stream clear index
    ctx.emitter.instruction(&format!("b {}", zero));                            // keep clearing the z_stream
    ctx.emitter.label(zeroed);

    // -- initialize the raw DEFLATE decoder --
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer
    ctx.emitter.instruction("mov x1, #-15");                                    // request raw inflate with negative window bits
    abi::emit_symbol_address(ctx.emitter, "x2", "_zlib_version");
    ctx.emitter.instruction("mov x3, #112");                                    // pass sizeof(z_stream)
    ctx.emitter.bl_c("inflateInit2_");                                          // initialize the raw-inflate zlib stream
    ctx.emitter.instruction("cmp w0, #0");                                      // check initialization before using zlib state
    ctx.emitter.instruction(&format!("b.ne {}", fail));                         // free the output buffer if initialization failed
    // -- bind the borrowed input and owned output to z_stream --
    ctx.emitter.instruction("ldr x9, [sp, #112]");                              // reload the source pointer
    ctx.emitter.instruction("str x9, [sp, #0]");                                // set z_stream.next_in
    ctx.emitter.instruction("ldr x9, [sp, #120]");                              // reload the source length
    ctx.emitter.instruction("str w9, [sp, #8]");                                // set z_stream.avail_in
    ctx.emitter.instruction("ldr x9, [sp, #128]");                              // reload the destination pointer
    ctx.emitter.instruction("str x9, [sp, #24]");                               // set z_stream.next_out
    ctx.emitter.instruction("ldr x9, [sp, #144]");                              // reload the destination capacity
    ctx.emitter.instruction("str w9, [sp, #32]");                               // set z_stream.avail_out
    // -- inflate until completion or exhaustion of the output buffer --
    ctx.emitter.label(&again);
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer to inflate
    ctx.emitter.instruction("mov x1, #4");                                      // request a final inflate pass
    ctx.emitter.bl_c("inflate");                                                // decompress the full input
    ctx.emitter.instruction("str x0, [sp, #136]");                              // save the inflate status code
    ctx.emitter.instruction("ldr x2, [sp, #40]");                               // read z_stream.total_out
    ctx.emitter.instruction("str x2, [sp, #152]");                              // save the inflated length across inflateEnd
    ctx.emitter.instruction("cmp w0, #1");                                      // accept a completed DEFLATE stream
    ctx.emitter.instruction(&format!("b.eq {}", finish));                       // release zlib state on success
    ctx.emitter.instruction("cmp w0, #0");                                      // Z_OK can require another output buffer
    ctx.emitter.instruction(&format!("b.eq {}", grow));                         // check whether the output buffer is full
    ctx.emitter.instruction("cmn w0, #5");                                      // Z_BUF_ERROR can also signal exhausted output
    ctx.emitter.instruction(&format!("b.ne {}", finish));                       // preserve other decompression failures
    // -- grow exhausted output storage while retaining zlib state and decoded bytes --
    ctx.emitter.label(&grow);
    ctx.emitter.instruction("ldr w9, [sp, #32]");                               // inspect the remaining output space
    ctx.emitter.instruction(&format!("cbnz w9, {}", finish));                   // a non-full buffer means growth cannot help
    ctx.emitter.instruction("ldr x9, [sp, #144]");                              // reload the current output capacity
    ctx.emitter.instruction("lsr x10, x9, #31");                                // keep the doubled capacity within the zlib u32 range
    ctx.emitter.instruction(&format!("cbnz x10, {}", finish));                  // stop if another doubling cannot be represented
    ctx.emitter.instruction("lsl x0, x9, #1");                                  // double the output capacity
    ctx.emitter.instruction("str x0, [sp, #144]");                              // save the expanded output capacity
    ctx.emitter.instruction("bl __rt_heap_alloc");                              // allocate replacement storage before releasing the old buffer
    ctx.emitter.instruction("mov x9, #1");                                      // heap kind 1 is an owned string
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // mark the replacement as releasable string storage
    ctx.emitter.instruction("str x0, [sp, #160]");                              // keep the replacement pointer across the copy
    ctx.emitter.instruction("ldr x1, [sp, #128]");                              // copy from the old output buffer
    ctx.emitter.instruction("ldr x2, [sp, #152]");                              // preserve every byte zlib already produced
    ctx.emitter.bl_c("memcpy");                                               // copy the decompressed prefix into replacement storage
    ctx.emitter.instruction("ldr x0, [sp, #128]");                              // release the superseded output buffer
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction("ldr x9, [sp, #160]");                              // reload the replacement output pointer
    ctx.emitter.instruction("str x9, [sp, #128]");                              // make replacement storage the sole output owner
    ctx.emitter.instruction("ldr x10, [sp, #152]");                             // reload the number of produced bytes
    ctx.emitter.instruction("add x9, x9, x10");                                 // resume output after the copied prefix
    ctx.emitter.instruction("str x9, [sp, #24]");                               // update z_stream.next_out after relocation
    ctx.emitter.instruction("ldr x9, [sp, #144]");                              // reload the expanded capacity
    ctx.emitter.instruction("sub x9, x9, x10");                                 // compute space after the preserved prefix
    ctx.emitter.instruction("str w9, [sp, #32]");                               // update z_stream.avail_out for the next pass
    ctx.emitter.instruction(&format!("b {}", again));                           // continue the same inflate stream
    // -- release zlib state before transferring or freeing the output owner --
    ctx.emitter.label(&finish);
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer to inflateEnd
    ctx.emitter.bl_c("inflateEnd");                                             // release zlib's internal inflate state
    ctx.emitter.instruction("ldr x9, [sp, #136]");                              // reload the inflate status code
    ctx.emitter.instruction("cmp x9, #1");                                      // check for Z_STREAM_END success
    ctx.emitter.instruction(&format!("b.ne {}", fail));                         // return false for zlib inflate failures
    ctx.emitter.instruction("ldr x1, [sp, #128]");                              // return the decompressed string pointer
    ctx.emitter.instruction("ldr x2, [sp, #152]");                              // return the decompressed string length
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the failure sentinel after success
    // -- release unpublished output storage on every failure --
    ctx.emitter.label(fail);
    ctx.emitter.instruction("ldr x0, [sp, #128]");                              // release the unpublished output buffer on failure
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction("mov x1, #0");                                      // use a null pointer as the failure sentinel
    ctx.emitter.instruction("mov x2, #0");                                      // clear the failure string length
    // -- restore the caller stack after success or failure --
    ctx.emitter.label(done);
    ctx.emitter.instruction("add sp, sp, #176");                                // release z_stream scratch storage
}

/// Emits x86_64 `gzinflate()` inline raw-inflate calls.
pub(super) fn lower_gzinflate_x86_64(
    ctx: &mut FunctionContext<'_>,
    zero: &str,
    zeroed: &str,
    fail: &str,
    done: &str,
) {
    let again = format!("{}_inflate_again", zero);
    let grow = format!("{}_inflate_grow", zero);
    let finish = format!("{}_inflate_finish", zero);
    let sized = format!("{}_sized", zero);
    // -- save the source and allocate initial output storage --
    ctx.emitter.instruction("sub rsp, 176");                                    // reserve z_stream storage plus scratch slots
    ctx.emitter.instruction("mov QWORD PTR [rsp + 112], rax");                  // save the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 120], rdx");                  // save the source length
    ctx.emitter.instruction("mov r9, rdx");                                     // copy the compressed byte length
    ctx.emitter.instruction("shl r9, 8");                                       // budget 256x the compressed byte length
    ctx.emitter.instruction("cmp r9, 65536");                                   // compare the scaled budget against the minimum
    ctx.emitter.instruction(&format!("jge {}", sized));                         // keep the scaled size when it is large enough
    ctx.emitter.instruction("mov r9, 65536");                                   // otherwise use the minimum inflate buffer size
    ctx.emitter.label(&sized);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 144], r9");                   // save the output capacity
    ctx.emitter.instruction("mov rax, r9");                                     // pass the output capacity to the heap allocator
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate the decompressed-data buffer
    ctx.emitter.instruction(
        &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))
    );                                                                          // materialize the x86_64 string heap kind word
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp the output buffer as a heap string
    ctx.emitter.instruction("mov QWORD PTR [rsp + 128], rax");                  // save the destination buffer pointer

    // -- zero z_stream before initializing its allocator callbacks --
    ctx.emitter.instruction("xor r9, r9");                                      // initialize the z_stream clear index
    ctx.emitter.label(zero);
    ctx.emitter.instruction("cmp r9, 112");                                     // check whether every z_stream byte is cleared
    ctx.emitter.instruction(&format!("jge {}", zeroed));                        // continue after the z_stream has been zeroed
    ctx.emitter.instruction("mov BYTE PTR [rsp + r9], 0");                      // clear one z_stream byte
    ctx.emitter.instruction("inc r9");                                          // advance the z_stream clear index
    ctx.emitter.instruction(&format!("jmp {}", zero));                          // keep clearing the z_stream
    ctx.emitter.label(zeroed);

    // -- initialize the raw DEFLATE decoder --
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer
    ctx.emitter.instruction("mov esi, -15");                                    // request raw inflate with negative window bits
    abi::emit_symbol_address(ctx.emitter, "rdx", "_zlib_version");
    ctx.emitter.instruction("mov ecx, 112");                                    // pass sizeof(z_stream)
    ctx.emitter.instruction("call inflateInit2_");                              // initialize the raw-inflate zlib stream
    ctx.emitter.instruction("test eax, eax");                                   // check initialization before using zlib state
    ctx.emitter.instruction(&format!("jnz {}", fail));                          // free the output buffer if initialization failed
    // -- bind the borrowed input and owned output to z_stream --
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 112]");                   // reload the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");                     // set z_stream.next_in
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                   // reload the source length
    ctx.emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                    // set z_stream.avail_in
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                   // reload the destination pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], r9");                    // set z_stream.next_out
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 144]");                   // reload the destination capacity
    ctx.emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                   // set z_stream.avail_out
    // -- inflate until completion or exhaustion of the output buffer --
    ctx.emitter.label(&again);
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer to inflate
    ctx.emitter.instruction("mov esi, 4");                                      // request a final inflate pass
    ctx.emitter.instruction("call inflate");                                    // decompress the full input
    ctx.emitter.instruction("mov QWORD PTR [rsp + 136], rax");                  // save the inflate status code
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // read z_stream.total_out
    ctx.emitter.instruction("mov QWORD PTR [rsp + 152], rax");                  // save the inflated length across inflateEnd
    ctx.emitter.instruction("cmp DWORD PTR [rsp + 136], 1");                    // accept a completed DEFLATE stream
    ctx.emitter.instruction(&format!("je {}", finish));                         // release zlib state on success
    ctx.emitter.instruction("cmp DWORD PTR [rsp + 136], 0");                    // Z_OK can require another output buffer
    ctx.emitter.instruction(&format!("je {}", grow));                           // check whether the output buffer is full
    ctx.emitter.instruction("cmp DWORD PTR [rsp + 136], -5");                   // Z_BUF_ERROR can signal exhausted output
    ctx.emitter.instruction(&format!("jne {}", finish));                        // preserve other decompression failures
    // -- grow exhausted output storage while retaining zlib state and decoded bytes --
    ctx.emitter.label(&grow);
    ctx.emitter.instruction("cmp DWORD PTR [rsp + 32], 0");                     // inspect the remaining output space
    ctx.emitter.instruction(&format!("jne {}", finish));                        // a non-full buffer means growth cannot help
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 144]");                  // reload the current output capacity
    ctx.emitter.instruction("mov r9, rax");                                     // copy capacity for the overflow check
    ctx.emitter.instruction("shr r9, 31");                                      // keep the doubled capacity within the zlib u32 range
    ctx.emitter.instruction(&format!("jnz {}", finish));                        // stop if another doubling cannot be represented
    ctx.emitter.instruction("shl rax, 1");                                      // double the output capacity
    ctx.emitter.instruction("mov QWORD PTR [rsp + 144], rax");                  // save the expanded output capacity
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate replacement storage before releasing the old buffer
    ctx.emitter.instruction(
        &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))
    );                                                                          // materialize the owned string marker
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // mark the replacement as releasable string storage
    ctx.emitter.instruction("mov QWORD PTR [rsp + 160], rax");                  // keep the replacement pointer across the copy
    ctx.emitter.instruction("mov rdi, rax");                                    // destination is the replacement output buffer
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 128]");                  // copy from the old output buffer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 152]");                  // preserve every byte zlib already produced
    ctx.emitter.instruction("call memcpy");                                     // copy the decompressed prefix into replacement storage
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                  // release the superseded output buffer
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 160]");                   // reload the replacement output pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 128], r9");                   // make replacement storage the sole output owner
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 152]");                  // reload the number of produced bytes
    ctx.emitter.instruction("add r9, r10");                                     // resume output after the copied prefix
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], r9");                    // update z_stream.next_out after relocation
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 144]");                   // reload the expanded capacity
    ctx.emitter.instruction("sub r9, r10");                                     // compute space after the preserved prefix
    ctx.emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                   // update z_stream.avail_out for the next pass
    ctx.emitter.instruction(&format!("jmp {}", again));                         // continue the same inflate stream
    // -- release zlib state before transferring or freeing the output owner --
    ctx.emitter.label(&finish);
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer to inflateEnd
    ctx.emitter.instruction("call inflateEnd");                                 // release zlib's internal inflate state
    ctx.emitter.instruction("cmp QWORD PTR [rsp + 136], 1");                    // check for Z_STREAM_END success
    ctx.emitter.instruction(&format!("jne {}", fail));                          // return false for zlib inflate failures
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                  // return the decompressed string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 152]");                  // return the decompressed string length
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the failure sentinel after success
    // -- release unpublished output storage on every failure --
    ctx.emitter.label(fail);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                  // release the unpublished output buffer on failure
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction("xor eax, eax");                                    // use a null pointer as the failure sentinel
    ctx.emitter.instruction("xor edx, edx");                                    // clear the failure string length
    // -- restore the caller stack after success or failure --
    ctx.emitter.label(done);
    ctx.emitter.instruction("add rsp, 176");                                    // release z_stream scratch storage
}
