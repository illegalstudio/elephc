//! Purpose:
//! Lowers gzip/zlib string builtins for AArch64 and x86_64.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Optional levels, output buffers, failure boxing, and stack cleanup stay target-aware.

use super::*;

mod inflate;

/// Lowers `gzcompress(data, level?)` through inline zlib `compress2` calls.
pub(crate) fn lower_gzcompress(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "gzcompress expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    load_single_gz_string_arg(ctx, inst, "gzcompress")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_gzcompress_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_gzcompress_x86_64(ctx, inst)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `gzdeflate(data, level?)` through inline raw-DEFLATE zlib calls.
pub(crate) fn lower_gzdeflate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "gzdeflate expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    load_single_gz_string_arg(ctx, inst, "gzdeflate")?;
    let zero = ctx.next_label("gzdeflate_zero");
    let zeroed = ctx.next_label("gzdeflate_zeroed");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_gzdeflate_aarch64(ctx, inst, &zero, &zeroed)?,
        Arch::X86_64 => lower_gzdeflate_x86_64(ctx, inst, &zero, &zeroed)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `gzinflate(data, max_length?)` and boxes zlib failures as PHP false.
pub(crate) fn lower_gzinflate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "gzinflate expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    load_single_gz_string_arg(ctx, inst, "gzinflate")?;
    let zero = ctx.next_label("gzinflate_zero");
    let zeroed = ctx.next_label("gzinflate_zeroed");
    let fail = ctx.next_label("gzinflate_fail");
    let done = ctx.next_label("gzinflate_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => inflate::lower_gzinflate_aarch64(ctx, &zero, &zeroed, &fail, &done),
        Arch::X86_64 => inflate::lower_gzinflate_x86_64(ctx, &zero, &zeroed, &fail, &done),
    }
    box_owned_string_or_false_result(ctx, "gzinflate");
    store_if_result(ctx, inst)
}

/// Lowers `gzuncompress(data, max_length?)` and boxes zlib failures as PHP false.
pub(crate) fn lower_gzuncompress(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "gzuncompress expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    load_single_gz_string_arg(ctx, inst, "gzuncompress")?;
    let ok = ctx.next_label("gzuncompress_ok");
    let after = ctx.next_label("gzuncompress_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_gzuncompress_aarch64(ctx, &ok, &after),
        Arch::X86_64 => lower_gzuncompress_x86_64(ctx, &ok, &after),
    }
    box_string_or_false_result(ctx, "gzuncompress");
    store_if_result(ctx, inst)
}
/// Loads the first gzip/zlib string operand into the target string-result registers.
pub(super) fn load_single_gz_string_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)
}

/// Materializes the optional zlib compression level for AArch64 gzip builtins.
pub(super) fn materialize_gz_level_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the compression level
    if inst.operands.len() >= 2 {
        let level = expect_operand(inst, 1)?;
        load_as_int(ctx, level, name)?;
    } else {
        ctx.emitter.instruction("mov x0, #-1");                                 // use zlib's default compression level when omitted
    }
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string after materializing the level
    Ok(())
}

/// Materializes the optional zlib compression level for x86_64 gzip builtins.
pub(super) fn materialize_gz_level_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    if inst.operands.len() >= 2 {
        let level = expect_operand(inst, 1)?;
        load_as_int(ctx, level, name)?;
    } else {
        ctx.emitter.instruction("mov eax, -1");                                 // use zlib's default compression level when omitted
    }
    ctx.emitter.instruction("mov rdi, rax");                                    // hold the compression level while restoring the source string
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    Ok(())
}

/// Emits AArch64 `gzcompress()` inline zlib calls.
pub(super) fn lower_gzcompress_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    materialize_gz_level_aarch64(ctx, inst, "gzcompress level")?;
    ctx.emitter.instruction("sub sp, sp, #64");                                 // reserve scratch storage for zlib compress2 state
    ctx.emitter.instruction("str x0, [sp, #0]");                                // save the compression level
    ctx.emitter.instruction("str x1, [sp, #8]");                                // save the source pointer
    ctx.emitter.instruction("str x2, [sp, #16]");                               // save the source length
    ctx.emitter.instruction("mov x0, x2");                                      // pass the source length to compressBound
    ctx.emitter.bl_c("compressBound");                                          // compute the worst-case compressed byte length
    ctx.emitter.instruction("str x0, [sp, #24]");                               // seed destLen with the output capacity
    ctx.emitter.instruction("bl __rt_heap_alloc");                              // allocate the compressed-data buffer
    ctx.emitter.instruction("mov x9, #1");                                      // heap kind 1 = owned string
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp the output buffer as a heap string
    ctx.emitter.instruction("str x0, [sp, #32]");                               // save the destination buffer pointer
    ctx.emitter.instruction("add x1, sp, #24");                                 // pass &destLen as the compress2 in/out length
    ctx.emitter.instruction("ldr x2, [sp, #8]");                                // pass the source pointer
    ctx.emitter.instruction("ldr x3, [sp, #16]");                               // pass the source length
    ctx.emitter.instruction("ldr x4, [sp, #0]");                                // pass the requested compression level
    ctx.emitter.bl_c("compress2");                                              // zlib-compress the source into the output buffer
    ctx.emitter.instruction("ldr x1, [sp, #32]");                               // return the compressed string pointer
    ctx.emitter.instruction("ldr x2, [sp, #24]");                               // return the compressed string length
    ctx.emitter.instruction("add sp, sp, #64");                                 // release the zlib scratch storage
    Ok(())
}

/// Emits x86_64 `gzcompress()` inline zlib calls.
pub(super) fn lower_gzcompress_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    materialize_gz_level_x86_64(ctx, inst, "gzcompress level")?;
    ctx.emitter.instruction("sub rsp, 64");                                     // reserve scratch storage for zlib compress2 state
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                    // save the compression level
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                    // save the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                   // save the source length
    ctx.emitter.instruction("mov rdi, rdx");                                    // pass the source length to compressBound
    ctx.emitter.instruction("call compressBound");                              // compute the worst-case compressed byte length
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");                   // seed destLen with the output capacity
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate the compressed-data buffer
    ctx.emitter.instruction(
        &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))
    );                                                                          // materialize the x86_64 string heap kind word
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp the output buffer as a heap string
    ctx.emitter.instruction("mov QWORD PTR [rsp + 32], rax");                   // save the destination buffer pointer
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the destination buffer pointer
    ctx.emitter.instruction("lea rsi, [rsp + 24]");                             // pass &destLen as the compress2 in/out length
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // pass the source pointer
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                   // pass the source length
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 0]");                     // pass the requested compression level
    ctx.emitter.instruction("call compress2");                                  // zlib-compress the source into the output buffer
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                   // return the compressed string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");                   // return the compressed string length
    ctx.emitter.instruction("add rsp, 64");                                     // release the zlib scratch storage
    Ok(())
}

/// Emits AArch64 `gzdeflate()` inline raw-deflate calls.
pub(super) fn lower_gzdeflate_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    zero: &str,
    zeroed: &str,
) -> Result<()> {
    materialize_gz_level_aarch64(ctx, inst, "gzdeflate level")?;
    ctx.emitter.instruction("sub sp, sp, #160");                                // reserve z_stream storage plus scratch slots
    ctx.emitter.instruction("str x0, [sp, #136]");                              // save the compression level
    ctx.emitter.instruction("str x1, [sp, #112]");                              // save the source pointer
    ctx.emitter.instruction("str x2, [sp, #120]");                              // save the source length
    ctx.emitter.instruction("mov x0, x2");                                      // pass the source length to compressBound
    ctx.emitter.bl_c("compressBound");                                          // compute the worst-case compressed byte length
    ctx.emitter.instruction("str x0, [sp, #144]");                              // save the output capacity
    ctx.emitter.instruction("bl __rt_heap_alloc");                              // allocate the compressed-data buffer
    ctx.emitter.instruction("mov x9, #1");                                      // heap kind 1 = owned string
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp the output buffer as a heap string
    ctx.emitter.instruction("str x0, [sp, #128]");                              // save the destination buffer pointer

    ctx.emitter.instruction("mov x9, #0");                                      // initialize the z_stream clear index
    ctx.emitter.label(zero);
    ctx.emitter.instruction("cmp x9, #112");                                    // check whether every z_stream byte is cleared
    ctx.emitter.instruction(&format!("b.ge {}", zeroed));                       // continue after the z_stream has been zeroed
    ctx.emitter.instruction("strb wzr, [sp, x9]");                              // clear one z_stream byte
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance the z_stream clear index
    ctx.emitter.instruction(&format!("b {}", zero));                            // keep clearing the z_stream
    ctx.emitter.label(zeroed);

    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer
    ctx.emitter.instruction("ldr x1, [sp, #136]");                              // pass the compression level
    ctx.emitter.instruction("mov x2, #8");                                      // pass Z_DEFLATED
    ctx.emitter.instruction("mov x3, #-15");                                    // request raw deflate with negative window bits
    ctx.emitter.instruction("mov x4, #8");                                      // pass zlib's default memory level
    ctx.emitter.instruction("mov x5, #0");                                      // pass Z_DEFAULT_STRATEGY
    abi::emit_symbol_address(ctx.emitter, "x6", "_zlib_version");
    ctx.emitter.instruction("mov x7, #112");                                    // pass sizeof(z_stream)
    ctx.emitter.bl_c("deflateInit2_");                                          // initialize the raw-deflate zlib stream
    ctx.emitter.instruction("ldr x9, [sp, #112]");                              // reload the source pointer
    ctx.emitter.instruction("str x9, [sp, #0]");                                // set z_stream.next_in
    ctx.emitter.instruction("ldr x9, [sp, #120]");                              // reload the source length
    ctx.emitter.instruction("str w9, [sp, #8]");                                // set z_stream.avail_in
    ctx.emitter.instruction("ldr x9, [sp, #128]");                              // reload the destination pointer
    ctx.emitter.instruction("str x9, [sp, #24]");                               // set z_stream.next_out
    ctx.emitter.instruction("ldr x9, [sp, #144]");                              // reload the destination capacity
    ctx.emitter.instruction("str w9, [sp, #32]");                               // set z_stream.avail_out
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer to deflate
    ctx.emitter.instruction("mov x1, #4");                                      // request a final deflate pass
    ctx.emitter.bl_c("deflate");                                                // compress the full input
    ctx.emitter.instruction("ldr x2, [sp, #40]");                               // read z_stream.total_out
    ctx.emitter.instruction("str x2, [sp, #152]");                              // save the compressed length across deflateEnd
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer to deflateEnd
    ctx.emitter.bl_c("deflateEnd");                                             // release zlib's internal deflate state
    ctx.emitter.instruction("ldr x1, [sp, #128]");                              // return the compressed string pointer
    ctx.emitter.instruction("ldr x2, [sp, #152]");                              // return the compressed string length
    ctx.emitter.instruction("add sp, sp, #160");                                // release z_stream scratch storage
    Ok(())
}

/// Emits x86_64 `gzdeflate()` inline raw-deflate calls.
pub(super) fn lower_gzdeflate_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    zero: &str,
    zeroed: &str,
) -> Result<()> {
    materialize_gz_level_x86_64(ctx, inst, "gzdeflate level")?;
    ctx.emitter.instruction("sub rsp, 160");                                    // reserve z_stream storage plus scratch slots
    ctx.emitter.instruction("mov QWORD PTR [rsp + 136], rdi");                  // save the compression level
    ctx.emitter.instruction("mov QWORD PTR [rsp + 112], rsi");                  // save the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 120], rdx");                  // save the source length
    ctx.emitter.instruction("mov rdi, rdx");                                    // pass the source length to compressBound
    ctx.emitter.instruction("call compressBound");                              // compute the worst-case compressed byte length
    ctx.emitter.instruction("mov QWORD PTR [rsp + 144], rax");                  // save the output capacity
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate the compressed-data buffer
    ctx.emitter.instruction(
        &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))
    );                                                                          // materialize the x86_64 string heap kind word
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp the output buffer as a heap string
    ctx.emitter.instruction("mov QWORD PTR [rsp + 128], rax");                  // save the destination buffer pointer

    ctx.emitter.instruction("xor r9, r9");                                      // initialize the z_stream clear index
    ctx.emitter.label(zero);
    ctx.emitter.instruction("cmp r9, 112");                                     // check whether every z_stream byte is cleared
    ctx.emitter.instruction(&format!("jge {}", zeroed));                        // continue after the z_stream has been zeroed
    ctx.emitter.instruction("mov BYTE PTR [rsp + r9], 0");                      // clear one z_stream byte
    ctx.emitter.instruction("inc r9");                                          // advance the z_stream clear index
    ctx.emitter.instruction(&format!("jmp {}", zero));                          // keep clearing the z_stream
    ctx.emitter.label(zeroed);

    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 136]");                  // pass the compression level
    ctx.emitter.instruction("mov edx, 8");                                      // pass Z_DEFLATED
    ctx.emitter.instruction("mov ecx, -15");                                    // request raw deflate with negative window bits
    ctx.emitter.instruction("mov r8d, 8");                                      // pass zlib's default memory level
    ctx.emitter.instruction("xor r9d, r9d");                                    // pass Z_DEFAULT_STRATEGY
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve stack slots for the last deflateInit2_ args
    abi::emit_symbol_address(ctx.emitter, "rax", "_zlib_version");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");                    // pass the zlib version string on the stack
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 112");                    // pass sizeof(z_stream) on the stack
    ctx.emitter.instruction("call deflateInit2_");                              // initialize the raw-deflate zlib stream
    ctx.emitter.instruction("add rsp, 16");                                     // release deflateInit2_ stack arguments
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 112]");                   // reload the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");                     // set z_stream.next_in
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                   // reload the source length
    ctx.emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                    // set z_stream.avail_in
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                   // reload the destination pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], r9");                    // set z_stream.next_out
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 144]");                   // reload the destination capacity
    ctx.emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                   // set z_stream.avail_out
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer to deflate
    ctx.emitter.instruction("mov esi, 4");                                      // request a final deflate pass
    ctx.emitter.instruction("call deflate");                                    // compress the full input
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // read z_stream.total_out
    ctx.emitter.instruction("mov QWORD PTR [rsp + 152], rax");                  // save the compressed length across deflateEnd
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer to deflateEnd
    ctx.emitter.instruction("call deflateEnd");                                 // release zlib's internal deflate state
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                  // return the compressed string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 152]");                  // return the compressed string length
    ctx.emitter.instruction("add rsp, 160");                                    // release z_stream scratch storage
    Ok(())
}

/// Emits AArch64 `gzuncompress()` inline zlib-wrapped inflate calls.
pub(super) fn lower_gzuncompress_aarch64(ctx: &mut FunctionContext<'_>, ok: &str, after: &str) {
    ctx.emitter.instruction("sub sp, sp, #48");                                 // reserve scratch storage for zlib uncompress state
    ctx.emitter.instruction("str x1, [sp, #0]");                                // save the source pointer
    ctx.emitter.instruction("str x2, [sp, #8]");                                // save the source length
    ctx.emitter.instruction("lsl x9, x2, #8");                                  // budget 256x the compressed byte length
    ctx.emitter.instruction("mov x10, #65536");                                 // materialize the minimum uncompress buffer size
    ctx.emitter.instruction("cmp x9, x10");                                     // compare the scaled budget against the minimum
    ctx.emitter.instruction("csel x9, x9, x10, gt");                            // choose the larger output buffer size
    ctx.emitter.instruction("str x9, [sp, #16]");                               // seed destLen with the output capacity
    ctx.emitter.instruction("mov x0, x9");                                      // pass the output capacity to the heap allocator
    ctx.emitter.instruction("bl __rt_heap_alloc");                              // allocate the decompressed-data buffer
    ctx.emitter.instruction("mov x9, #1");                                      // heap kind 1 = owned string
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp the output buffer as a heap string
    ctx.emitter.instruction("str x0, [sp, #24]");                               // save the destination buffer pointer
    ctx.emitter.instruction("add x1, sp, #16");                                 // pass &destLen as the uncompress in/out length
    ctx.emitter.instruction("ldr x2, [sp, #0]");                                // pass the source pointer
    ctx.emitter.instruction("ldr x3, [sp, #8]");                                // pass the source length
    ctx.emitter.bl_c("uncompress");                                             // zlib-uncompress the source
    ctx.emitter.instruction(&format!("cbz x0, {}", ok));                        // zero zlib status means success
    ctx.emitter.instruction("mov x1, #0");                                      // use a null pointer as the failure sentinel
    ctx.emitter.instruction("mov x2, #0");                                      // clear the failure string length
    ctx.emitter.instruction(&format!("b {}", after));                           // skip the success result after failure
    ctx.emitter.label(ok);
    ctx.emitter.instruction("ldr x1, [sp, #24]");                               // return the decompressed string pointer
    ctx.emitter.instruction("ldr x2, [sp, #16]");                               // return the decompressed string length
    ctx.emitter.label(after);
    ctx.emitter.instruction("add sp, sp, #48");                                 // release the zlib scratch storage
}

/// Emits x86_64 `gzuncompress()` inline zlib-wrapped inflate calls.
pub(super) fn lower_gzuncompress_x86_64(ctx: &mut FunctionContext<'_>, ok: &str, after: &str) {
    let sized = ctx.next_label("gzuncompress_sized");
    ctx.emitter.instruction("sub rsp, 48");                                     // reserve scratch storage for zlib uncompress state
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");                    // save the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // save the source length
    ctx.emitter.instruction("mov r9, rdx");                                     // copy the compressed byte length
    ctx.emitter.instruction("shl r9, 8");                                       // budget 256x the compressed byte length
    ctx.emitter.instruction("cmp r9, 65536");                                   // compare the scaled budget against the minimum
    ctx.emitter.instruction(&format!("jge {}", sized));                         // keep the scaled size when it is large enough
    ctx.emitter.instruction("mov r9, 65536");                                   // otherwise use the minimum uncompress buffer size
    ctx.emitter.label(&sized);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r9");                    // seed destLen with the output capacity
    ctx.emitter.instruction("mov rax, r9");                                     // pass the output capacity to the heap allocator
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate the decompressed-data buffer
    ctx.emitter.instruction(
        &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))
    );                                                                          // materialize the x86_64 string heap kind word
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp the output buffer as a heap string
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");                   // save the destination buffer pointer
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the destination buffer pointer
    ctx.emitter.instruction("lea rsi, [rsp + 16]");                             // pass &destLen as the uncompress in/out length
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 0]");                    // pass the source pointer
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");                    // pass the source length
    ctx.emitter.instruction("call uncompress");                                 // zlib-uncompress the source
    ctx.emitter.instruction("test rax, rax");                                   // zero zlib status means success
    ctx.emitter.instruction(&format!("jz {}", ok));                             // load the success result for zero status
    ctx.emitter.instruction("xor eax, eax");                                    // use a null pointer as the failure sentinel
    ctx.emitter.instruction("xor edx, edx");                                    // clear the failure string length
    ctx.emitter.instruction(&format!("jmp {}", after));                         // skip the success result after failure
    ctx.emitter.label(ok);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // return the decompressed string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                   // return the decompressed string length
    ctx.emitter.label(after);
    ctx.emitter.instruction("add rsp, 48");                                     // release the zlib scratch storage
}
