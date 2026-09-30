//! Purpose:
//! Lowers the compiler-internal `expect_array_arg` runtime call: unbox an `array|false`
//! builtin argument, or throw php's TypeError for the `false`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch, for calls the ARGUMENT lowering
//!   inserts when an `array|false` union (scandir, glob, file…) flows into an array-taking
//!   builtin.
//!
//! Key details:
//! - The consumer's own lowering never sees the box: it receives a raw array pointer typed with
//!   the union's array member, so the ~forty array-taking lowerings stay untouched.
//! - The message is the second OPERAND — an ordinary compile-time string constant composed by
//!   the argument lowering (`{fn}(): Argument #{n} (${param}) must be of type array, false
//!   given`) — so no per-builtin symbol registration exists anywhere.

use super::*;

/// Unboxes a boxed `array|false` value or throws the TypeError the message operand spells.
pub(crate) fn lower_expect_array_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "expect_array_arg", 2)?;
    let boxed = expect_operand(inst, 0)?;
    let message = expect_operand(inst, 1)?;
    ctx.load_value_to_result(boxed)?;
    let unboxed = ctx.next_label("eaa_array");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed payload tag
            ctx.emitter.instruction("cmp x9, #4");                              // an indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", unboxed));
            ctx.emitter.instruction("cmp x9, #5");                              // or an assoc hash?
            ctx.emitter.instruction(&format!("b.eq {}", unboxed));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed payload tag
            ctx.emitter.instruction("cmp r9, 4");                               // an indexed array?
            ctx.emitter.instruction(&format!("je {}", unboxed));
            ctx.emitter.instruction("cmp r9, 5");                               // or an assoc hash?
            ctx.emitter.instruction(&format!("je {}", unboxed));
        }
    }
    // By typing the only other inhabitant is `false`, and the message operand already spells
    // it; the throw never returns.
    // php's frame #0 names the builtin the PROGRAM called, with the value that reached it. The
    // message operand spells that name — the argument lowering composed it — and `false` is the
    // only value that can be here, by typing.
    emit_consuming_builtin_trace_frame(ctx, inst, message)?;
    io::load_string_to_result(ctx, message, "expect_array_arg message")?;
    // php ends the report with ` in FILE:LINE` and writes the trace block after it. The message
    // is composed at run time; the location is this instruction's own, and without it the report
    // was one line where php writes five.
    let location = ctx
        .module
        .source_path
        .clone()
        .map(|file| (file, inst.span.map_or(0, |span| span.line)));
    super::exceptions::emit_type_error_from_string_result_at(ctx, location);
    ctx.emitter.label(&unboxed);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // the raw array pointer the consumer expects
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // the raw array pointer the consumer expects
        }
    }
    store_if_result(ctx, inst)
}

/// Opens php's frame #0 for the builtin whose argument this unwrap refused.
///
/// The name comes out of the message the argument lowering composed — `array_keys(): Argument #1
/// …` — so no second table of builtin names exists to drift from the first. The single argument is
/// the literal `false`, which is what the union's other member is.
fn emit_consuming_builtin_trace_frame(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    message: ValueId,
) -> Result<()> {
    let Some(text) = maybe_const_string_operand(ctx, message)? else {
        return Ok(());
    };
    let Some(name) = text.split("():").next().filter(|name| !name.is_empty()) else {
        return Ok(());
    };
    let line = inst.span.map_or(0, |span| span.line);
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    abi::emit_call_label(ctx.emitter, "__rt_trace_reset");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", i64::from(line));
            abi::emit_symbol_address(ctx.emitter, "x1", &name_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", name_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_trace_frame_open");
            abi::emit_load_int_immediate(ctx.emitter, "x0", 3);                 // runtime tag 3 = bool
            abi::emit_load_int_immediate(ctx.emitter, "x1", 0);                 // and its payload is false
            abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", i64::from(line));
            abi::emit_symbol_address(ctx.emitter, "rsi", &name_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", name_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_trace_frame_open");
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 3);                // runtime tag 3 = bool
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);                // and its payload is false
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_trace_arg");
    abi::emit_call_label(ctx.emitter, "__rt_trace_frame_close");
    Ok(())
}
