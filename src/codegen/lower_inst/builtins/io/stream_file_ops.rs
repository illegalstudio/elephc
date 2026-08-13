//! Purpose:
//! Stream close, read, write, formatted IO, and CSV calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `fclose(stream)` after validating and unboxing the stream handle.
pub(crate) fn lower_fclose(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fclose", 1)?;
    let stream = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, stream)?;
    load_stream_fd_to_result(ctx, stream, "fclose")?;
    apply_resource_release_sentinel(ctx, captured);
    let success_label = ctx.next_label("fclose_ok");
    let done_label = ctx.next_label("fclose_done");
    let user_wrapper_label = ctx.next_label("fclose_user_wrapper");
    let phar_label = ctx.next_label("fclose_phar");
    let not_phar_label = ctx.next_label("fclose_not_phar");
    let after_dispatch_label = ctx.next_label("fclose_after_dispatch");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x5000");                         // low half of the phar-write descriptor base 0x50000000
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the phar-write synthetic descriptor base
            ctx.emitter.instruction("cmp x0, x9");                              // is the descriptor below the phar-write range?
            ctx.emitter.instruction(&format!("b.lt {}", not_phar_label));       // below the PHAR range: continue with normal dispatch
            ctx.emitter.instruction("add x10, x9, #32");                        // upper bound for the 32 buffered PHAR write descriptors
            ctx.emitter.instruction("cmp x0, x10");                             // is this inside the phar-write descriptor range?
            ctx.emitter.instruction(&format!("b.lt {}", phar_label));           // finalize phar writes instead of closing a real fd
            ctx.emitter.label(&not_phar_label);
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this is a userspace-wrapper stream
            ctx.emitter.instruction(&format!("b.ge {}", user_wrapper_label));   // dispatch synthetic handles without indexing fd tables
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x50000000");                     // materialize the phar-write synthetic descriptor base
            ctx.emitter.instruction("cmp rax, r9");                             // is the descriptor below the phar-write range?
            ctx.emitter.instruction(&format!("jl {}", not_phar_label));         // below the PHAR range: continue with normal dispatch
            ctx.emitter.instruction("lea r10, [r9 + 32]");                      // upper bound for the 32 buffered PHAR write descriptors
            ctx.emitter.instruction("cmp rax, r10");                            // is this inside the phar-write descriptor range?
            ctx.emitter.instruction(&format!("jl {}", phar_label));             // finalize phar writes instead of closing a real fd
            ctx.emitter.label(&not_phar_label);
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this is a userspace-wrapper stream
            ctx.emitter.instruction(&format!("jge {}", user_wrapper_label));    // dispatch synthetic handles without indexing fd tables
        }
    }
    emit_zlib_flush_on_close_for_current_fd(ctx);
    emit_bz2_flush_on_close_for_current_fd(ctx);
    emit_iconv_flush_on_close_for_current_fd(ctx);
    emit_tls_session_teardown_for_current_fd(ctx);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the user-filter teardown helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_filter_release_fd");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear any read filter before the descriptor can be reused
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear any write filter before the descriptor can be reused
            ctx.emitter.syscall(6);
            ctx.emitter.instruction("cmp x0, #0");                              // test whether close() reported success
            ctx.emitter.instruction(&format!("b.eq {}", success_label));        // branch to the true result when the stream closed cleanly
            ctx.emitter.instruction("mov x0, #0");                              // return false when the stream close failed
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the success result on the failure path
            ctx.emitter.label(&success_label);
            ctx.emitter.instruction("mov x0, #1");                              // return true when the stream close succeeded
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear any read filter before the descriptor can be reused
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear any write filter before the descriptor can be reused
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream fd to libc close()
            ctx.emitter.instruction("call close");                              // close the requested stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether close() reported success
            ctx.emitter.instruction(&format!("je {}", success_label));          // branch to the true result when the stream closed cleanly
            ctx.emitter.instruction("xor eax, eax");                            // return false when the stream close failed
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the success result on the failure path
            ctx.emitter.label(&success_label);
            ctx.emitter.instruction("mov rax, 1");                              // return true when the stream close succeeded
        }
    }
    ctx.emitter.label(&done_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip synthetic close handlers after the native fd path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip synthetic close handlers after the native fd path
        }
    }
    ctx.emitter.label(&user_wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the close helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fclose");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip phar finalization after wrapper close dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip phar finalization after wrapper close dispatch
        }
    }
    ctx.emitter.label(&phar_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the PHAR write descriptor to the finalizer
    }
    abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
    ctx.emitter.label(&after_dispatch_label);
    store_if_result(ctx, inst)
}

/// Lowers `fread(stream, length)` using the shared runtime file-read helper.
pub(crate) fn lower_fread(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fread", 2)?;
    let stream = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fread")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(length)?.codegen_repr(), "fread length")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the requested byte count to the fread runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the requested byte count to the fread runtime helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    store_if_result(ctx, inst)
}

/// Lowers `fwrite(stream, data)` and boxes a byte count or PHP `false` on error.
pub(crate) fn lower_fwrite(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fwrite", 2)?;
    let stream = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fwrite")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, data, "fwrite data")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, data, "fwrite data")?;
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string pointer to the runtime fwrite helper
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
        }
    }
    box_negative_int_or_false_result(ctx, "fwrite");
    store_if_result(ctx, inst)
}

/// Lowers `fprintf(stream, format, values...)` as `sprintf()` plus stream write.
pub(crate) fn lower_fprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fprintf", 2, usize::MAX)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    let spec_cats = super::super::strings::sprintf_spec_cats_for_format(ctx, format)?;
    load_stream_fd_to_result(ctx, stream, "fprintf")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for index in (2..inst.operands.len()).rev() {
        let value = expect_operand(inst, index)?;
        let spec_cat = spec_cats.get(index - 2).copied();
        super::super::strings::pack_sprintf_like_arg(ctx, value, spec_cat, "fprintf")?;
    }
    load_string_to_result(ctx, format, "fprintf format")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", inst.operands.len() - 2)); // pass the number of packed fprintf operands
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", (inst.operands.len() - 2) as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_sprintf");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer to fwrite
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fwrite");
    store_if_result(ctx, inst)
}

/// Lowers `vfprintf(stream, format, values)` through `__rt_vsprintf` then fwrite.
pub(crate) fn lower_vfprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "vfprintf", 3)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    let values = expect_operand(inst, 2)?;
    load_stream_fd_to_result(ctx, stream, "vfprintf")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd and format scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // save the descriptor across formatting
            load_string_to_result(ctx, format, "vfprintf format")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #8]");                    // save the format pointer and length
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("ldp x1, x2, [sp, #8]");                    // restore the format pointer and length
            abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the destination descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
            ctx.emitter.instruction("add sp, sp, #32");                         // release vfprintf scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd and format scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the descriptor across formatting
            load_string_to_result(ctx, format, "vfprintf format")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // save the format pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");           // save the format byte length
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the values array to vsprintf
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // restore the format pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // restore the format byte length
            abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer to fwrite
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the destination descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
            ctx.emitter.instruction("add rsp, 32");                             // release vfprintf scratch storage
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fscanf(stream, format)` through `__rt_fgets` and `__rt_sscanf`.
pub(crate) fn lower_fscanf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fscanf", 2, usize::MAX)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fscanf")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_fgets");
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, format, "fscanf format")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the format pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the format length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the descriptor to fgets
            abi::emit_call_label(ctx.emitter, "__rt_fgets");
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, format, "fscanf format")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the format pointer as the secondary string argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the format length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_sscanf");
    store_if_result(ctx, inst)
}

/// Lowers `fgets(stream)` through the shared line-read runtime helper.
pub(crate) fn lower_fgets(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fgets", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fgets")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 fgets runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgets");
    box_stream_string_or_false_on_empty_result(ctx, "fgets");
    store_if_result(ctx, inst)
}

/// Lowers `fgetc(stream)` and boxes the one-byte string or PHP false result.
pub(crate) fn lower_fgetc(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fgetc", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fgetc")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 fgetc runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgetc");
    box_stream_string_or_false_on_empty_result(ctx, "fgetc");
    store_if_result(ctx, inst)
}

/// Lowers `fgetcsv(stream, separator?, enclosure?)` through the CSV row runtime helper.
pub(crate) fn lower_fgetcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fgetcsv", 1, 3)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fgetcsv")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 fgetcsv runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgetcsv");
    box_indexed_array_or_false_result(ctx);                                     // EOF is a null result, which PHP reports as false
    store_if_result(ctx, inst)
}

/// Lowers `fputcsv(stream, fields, separator?, enclosure?)` for string arrays.
pub(crate) fn lower_fputcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fputcsv", 2, 4)?;
    let stream = expect_operand(inst, 0)?;
    let fields = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fputcsv")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_string_array(ctx.load_value_to_result(fields)?.codegen_repr(), "fputcsv fields")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the string-array pointer to the fputcsv runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string-array pointer to the fputcsv runtime helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fputcsv");
    store_if_result(ctx, inst)
}

/// Lowers `fpassthru(stream)` through the remaining-bytes stream runtime helper.
pub(crate) fn lower_fpassthru(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "fpassthru", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fpassthru")?;
    emit_fpassthru_dispatch(ctx);
    store_if_result(ctx, inst)
}
