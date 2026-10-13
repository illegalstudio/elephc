//! Purpose:
//! Stream predicates, options, select, and include-path resolution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers descriptor duplication and release for the internal Async reactor.
pub(crate) fn lower_async_fd(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_async_fd", 2)?;
    let source = expect_operand(inst, 0)?;
    let operation = expect_operand(inst, 1)?;
    ctx.load_value_to_result(source)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(operation)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the descriptor ownership operation
            abi::emit_pop_reg(ctx.emitter, "x0");                               // pass the boxed source array
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the descriptor ownership operation
            abi::emit_pop_reg(ctx.emitter, "rdi");                              // pass the boxed source array
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_async_fd");
    store_if_result(ctx, inst)
}

/// Lowers the internal packed-descriptor poll used by the Async reactor.
pub(crate) fn lower_async_poll(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_async_poll", 2)?;
    let entries = expect_operand(inst, 0)?;
    let timeout_ms = expect_operand(inst, 1)?;
    ctx.load_value_to_result(entries)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(timeout_ms)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // timeout milliseconds
            abi::emit_pop_reg(ctx.emitter, "x0");                           // packed descriptor entries
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // timeout milliseconds
            abi::emit_pop_reg(ctx.emitter, "rdi");                          // packed descriptor entries
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_async_poll");
    store_if_result(ctx, inst)
}

/// Lowers `stream_isatty(stream)`.
pub(crate) fn lower_stream_isatty(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_isatty", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_isatty")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the runtime terminal probe
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_isatty");
    store_if_result(ctx, inst)
}

/// Lowers `stream_set_blocking(stream, enable)`.
pub(crate) fn lower_stream_set_blocking(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_set_blocking", 2)?;
    let stream = expect_operand(inst, 0)?;
    let enable = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_set_blocking")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int_or_bool(
        ctx.load_value_to_result(enable)?.codegen_repr(),
        "stream_set_blocking enable",
    )?;
    let wrapper = ctx.next_label("set_blocking_wrapper");
    let after = ctx.next_label("set_blocking_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the blocking flag as the native helper's second argument
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrapper));              // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_blocking");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov x2, x1");                              // pass the blocking flag as wrapper option arg1
            ctx.emitter.instruction(&format!("mov x1, #{}", STREAM_OPTION_BLOCKING)); // select STREAM_OPTION_BLOCKING
            ctx.emitter.instruction("mov x3, #0");                              // pass zero as wrapper option arg2
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the blocking flag as the native helper's second argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrapper));               // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_blocking");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdx, rsi");                            // pass the blocking flag as wrapper option arg1
            ctx.emitter.instruction(&format!("mov rsi, {}", STREAM_OPTION_BLOCKING)); // select STREAM_OPTION_BLOCKING
            ctx.emitter.instruction("xor ecx, ecx");                            // pass zero as wrapper option arg2
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_set_chunk_size(stream, size)` and returns the previous size.
pub(crate) fn lower_stream_set_chunk_size(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_set_chunk_size", 2)?;
    let stream = expect_operand(inst, 0)?;
    let size = expect_operand(inst, 1)?;
    let default_label = ctx.next_label("stream_chunk_default");
    let have_old_label = ctx.next_label("stream_chunk_have_old");
    let done_label = ctx.next_label("stream_chunk_done");
    load_stream_fd_to_result(ctx, stream, "stream_set_chunk_size")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(size)?.codegen_repr(),
        "stream_set_chunk_size size",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // keep the new chunk size while restoring the stream fd
            abi::emit_pop_reg(ctx.emitter, "x2");
            ctx.emitter.instruction("cmp x2, #0");                              // negative descriptors cannot index the chunk-size table
            ctx.emitter.instruction(&format!("b.lt {}", default_label));        // out-of-range descriptors report the default
            ctx.emitter.instruction("cmp x2, #256");                            // descriptors above the fixed table are ignored
            ctx.emitter.instruction(&format!("b.ge {}", default_label));        // out-of-range descriptors report the default
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_chunk_size");
            ctx.emitter.instruction("ldr x10, [x9, x2, lsl #3]");               // load the previous per-fd chunk size
            ctx.emitter.instruction(&format!("cbnz x10, {}", have_old_label));  // keep a previously stored size when present
            ctx.emitter.instruction("mov x10, #8192");                          // use PHP's default stream chunk size
            ctx.emitter.label(&have_old_label);
            ctx.emitter.instruction("str x1, [x9, x2, lsl #3]");                // store the new chunk size for this fd
            ctx.emitter.instruction("mov x0, x10");                             // return the previous chunk size
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the default-only path
            ctx.emitter.label(&default_label);
            ctx.emitter.instruction("mov x0, #8192");                           // report PHP's default chunk size
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // keep the new chunk size while restoring the stream fd
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("cmp rdi, 0");                              // negative descriptors cannot index the chunk-size table
            ctx.emitter.instruction(&format!("jl {}", default_label));          // out-of-range descriptors report the default
            ctx.emitter.instruction("cmp rdi, 256");                            // descriptors above the fixed table are ignored
            ctx.emitter.instruction(&format!("jge {}", default_label));         // out-of-range descriptors report the default
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_chunk_size");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9 + rdi * 8]");       // load the previous per-fd chunk size
            ctx.emitter.instruction("test rax, rax");                           // check whether a previous size exists
            ctx.emitter.instruction(&format!("jnz {}", have_old_label));        // keep a previously stored size when present
            ctx.emitter.instruction("mov eax, 8192");                           // use PHP's default stream chunk size
            ctx.emitter.label(&have_old_label);
            ctx.emitter.instruction("mov QWORD PTR [r9 + rdi * 8], rsi");       // store the new chunk size for this fd
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the default-only path
            ctx.emitter.label(&default_label);
            ctx.emitter.instruction("mov eax, 8192");                           // report PHP's default chunk size
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers stream read/write buffer setters as successful no-ops.
pub(crate) fn lower_stream_set_buffer(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_set_buffer", 2, 2)?;
    for operand in &inst.operands {
        ctx.load_value_to_result(*operand)?;
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    store_if_result(ctx, inst)
}

/// Lowers `stream_set_timeout(stream, seconds, microseconds?)`.
pub(crate) fn lower_stream_set_timeout(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_set_timeout", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let seconds = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_set_timeout")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(seconds)?.codegen_repr(),
        "stream_set_timeout seconds",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if inst.operands.len() == 3 {
                let usec = expect_operand(inst, 2)?;
                require_int(
                    ctx.load_value_to_result(usec)?.codegen_repr(),
                    "stream_set_timeout microseconds",
                )?;
                ctx.emitter.instruction("mov x2, x0");                          // pass explicit microseconds as the third runtime argument
            } else {
                ctx.emitter.instruction("mov x2, #0");                          // default omitted microseconds to zero
            }
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            if inst.operands.len() == 3 {
                let usec = expect_operand(inst, 2)?;
                require_int(
                    ctx.load_value_to_result(usec)?.codegen_repr(),
                    "stream_set_timeout microseconds",
                )?;
                ctx.emitter.instruction("mov rdx, rax");                        // pass explicit microseconds as the third runtime argument
            } else {
                ctx.emitter.instruction("xor edx, edx");                        // default omitted microseconds to zero
            }
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    lower_stream_timeout_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stream_select(read, write, except, seconds, microseconds?)`.
pub(crate) fn lower_stream_select(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_select", 4, 5)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    for idx in 0..4 {
        let value = expect_operand(inst, idx)?;
        ctx.load_value_to_result(value)?;
        abi::emit_push_reg(ctx.emitter, result_reg);
    }
    if inst.operands.len() == 5 {
        let microseconds = expect_operand(inst, 4)?;
        ctx.load_value_to_result(microseconds)?;
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #0");                          // default omitted microseconds to zero
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // default omitted microseconds to zero
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x4, x0");                              // pass microseconds as the fifth runtime argument
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, rax");                             // pass microseconds as the fifth runtime argument
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_select");
    store_if_result(ctx, inst)
}

/// Lowers `stream_resolve_include_path(filename)` as realpath-backed `string|false`.
pub(crate) fn lower_stream_resolve_include_path(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_resolve_include_path", 1)?;
    let filename = expect_operand(inst, 0)?;
    load_string_to_result(ctx, filename, "stream_resolve_include_path")?;
    abi::emit_call_label(ctx.emitter, "__rt_realpath");
    box_owned_string_or_false_result(ctx, "stream_resolve_include_path");
    store_if_result(ctx, inst)
}
