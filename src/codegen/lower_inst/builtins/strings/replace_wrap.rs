//! Purpose:
//! Lowers whole-string replacement, padding, and word wrapping builtins.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Optional pad, break, width, and mode arguments use target-specific ABI materialization.

use super::*;

/// Lowers `str_replace()`/`str_ireplace()` with three string operands.
pub(crate) fn lower_string_replace(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    if inst.operands.len() != 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 3 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_string_replace_aarch64(ctx, inst, name)?,
        Arch::X86_64 => lower_string_replace_x86_64(ctx, inst, name)?,
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `wordwrap(string, width?, break?, cut?)` through the shared runtime helper.
pub(crate) fn lower_wordwrap(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "wordwrap expected 1 to 4 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_wordwrap_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_wordwrap_x86_64(ctx, inst)?,
    }
    emit_wordwrap_argument_guards(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_wordwrap");
    store_if_result(ctx, inst)
}

/// Rejects the `wordwrap()` argument values reference PHP refuses to wrap with.
///
/// An empty `$break` gives the wrapper nothing to insert, so it silently returned the input
/// unwrapped where PHP raises a `ValueError`; a zero `$width` combined with `$cut_long_words`
/// asks for progress-free cutting. php-src checks `$break` first, then the width/cut pair.
fn emit_wordwrap_argument_guards(ctx: &mut FunctionContext<'_>) {
    let break_ok_label = ctx.next_label("wordwrap_break_ok");
    let width_ok_label = ctx.next_label("wordwrap_width_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x5, {}", break_ok_label));   // a non-empty break string can be inserted
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test r8, r8");                             // is the break string empty?
            ctx.emitter.instruction(&format!("jnz {}", break_ok_label));        // a non-empty break string can be inserted
        }
    }
    super::super::exceptions::emit_value_error(ctx, WORDWRAP_EMPTY_BREAK_MESSAGE);
    ctx.emitter.label(&break_ok_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x3, {}", width_ok_label));   // a non-zero width always makes progress
            ctx.emitter.instruction(&format!("cbz x6, {}", width_ok_label));    // a zero width is only rejected together with $cut_long_words
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdi, rdi");                           // is the requested wrap width zero?
            ctx.emitter.instruction(&format!("jnz {}", width_ok_label));        // a non-zero width always makes progress
            ctx.emitter.instruction("test r9, r9");                             // was $cut_long_words requested?
            ctx.emitter.instruction(&format!("jz {}", width_ok_label));         // a zero width is only rejected together with $cut_long_words
        }
    }
    super::super::exceptions::emit_value_error(ctx, WORDWRAP_ZERO_WIDTH_CUT_MESSAGE);
    ctx.emitter.label(&width_ok_label);
}

/// Lowers `base_convert(num, from_base, to_base)` through the shared runtime helper.
///
/// php-src validates `$from_base` first and `$to_base` second, before touching `$num`, so the
/// two guards are emitted in that order once both bases sit in their runtime argument
/// registers.
pub(crate) fn lower_base_convert(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "base_convert expected 3 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_base_convert_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_base_convert_x86_64(ctx, inst)?,
    }
    let (from_base_reg, to_base_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdx", "rcx"),
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedInRange(from_base_reg, 2, 36),
        BASE_CONVERT_FROM_BASE_MESSAGE,
    );
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedInRange(to_base_reg, 2, 36),
        BASE_CONVERT_TO_BASE_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, "__rt_base_convert");
    store_if_result(ctx, inst)
}

/// Materializes AArch64 `base_convert()` runtime arguments.
///
/// Both bases are materialized after the numeral, so each one is parked on the stack while
/// the next operand is lowered: `load_as_int` may call `__rt_str_to_int`, which clobbers
/// every scratch register the earlier arguments were sitting in.
fn lower_base_convert_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "base_convert", "x1", "x2")?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    let from_base = expect_operand(inst, 1)?;
    load_as_int(ctx, from_base, "base_convert from_base")?;
    abi::emit_push_reg_pair(ctx.emitter, "x0", "xzr");
    let to_base = expect_operand(inst, 2)?;
    load_as_int(ctx, to_base, "base_convert to_base")?;
    ctx.emitter.instruction("mov x4, x0");                                      // pass the target base to the runtime helper
    abi::emit_pop_reg_pair(ctx.emitter, "x3", "x9");
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    Ok(())
}

/// Materializes x86_64 `base_convert()` runtime arguments.
///
/// Same staging as the AArch64 path: the numeral and the source base wait on the stack until
/// the target base has been materialized, then everything lands in the System V registers
/// `__rt_base_convert` reads.
fn lower_base_convert_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "base_convert", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    let from_base = expect_operand(inst, 1)?;
    load_as_int(ctx, from_base, "base_convert from_base")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rax");
    let to_base = expect_operand(inst, 2)?;
    load_as_int(ctx, to_base, "base_convert to_base")?;
    ctx.emitter.instruction("mov rcx, rax");                                    // pass the target base to the runtime helper
    abi::emit_pop_reg_pair(ctx.emitter, "rdx", "r9");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    Ok(())
}

/// Lowers `chunk_split(string, length?, separator?)` through the shared runtime helper.
pub(crate) fn lower_chunk_split(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chunk_split_impl(ctx, inst, "__rt_chunk_split")
}

/// Lowers the length-only `strlen(chunk_split(...))` helper with the same argument
/// materialization and `$length >= 1` guard as the allocating form.
pub(crate) fn lower_chunk_split_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chunk_split_impl(ctx, inst, "__rt_chunk_split_len")
}

/// Shared `chunk_split` argument materialization, validation, and runtime call.
fn lower_chunk_split_impl(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    runtime_label: &str,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "chunk_split expected 1 to 3 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_chunk_split_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_chunk_split_x86_64(ctx, inst)?,
    }
    // `__rt_chunk_split` divides the subject length by the chunk length, so a zero length
    // would trap and a negative one would make the unsigned compare copy the whole subject
    // forever. Reference PHP rejects both before touching the subject.
    let length_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(length_reg, 1),
        CHUNK_SPLIT_NON_POSITIVE_LENGTH_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Materializes AArch64 `chunk_split()` runtime arguments.
fn lower_chunk_split_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "chunk_split")?;
    ctx.load_string_value_to_regs(subject, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject while materializing the length and separator
    if inst.operands.len() >= 2 {
        let length = expect_operand(inst, 1)?;
        load_as_int(ctx, length, "chunk_split length")?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the requested chunk length to the runtime helper
    } else {
        ctx.emitter.instruction("mov x3, #76");                                 // use PHP's default 76-byte chunk length when omitted
    }
    if inst.operands.len() >= 3 {
        let separator = expect_string_operand(ctx, inst, 2, "chunk_split")?;
        ctx.load_string_value_to_regs(separator, "x1", "x2")?;
        ctx.emitter.instruction("mov x4, x1");                                  // pass the separator pointer to the runtime helper
        ctx.emitter.instruction("mov x5, x2");                                  // pass the separator length to the runtime helper
    } else {
        let (label, len) = ctx.data.add_string(b"\r\n");
        abi::emit_symbol_address(ctx.emitter, "x4", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x5", len as i64);
    }
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the subject into the primary runtime argument registers
    Ok(())
}

/// Materializes x86_64 `chunk_split()` runtime arguments.
fn lower_chunk_split_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "chunk_split")?;
    ctx.load_string_value_to_regs(subject, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    if inst.operands.len() >= 2 {
        let length = expect_operand(inst, 1)?;
        load_as_int(ctx, length, "chunk_split length")?;
        ctx.emitter.instruction("mov rdi, rax");                                // pass the requested chunk length to the runtime helper
    } else {
        ctx.emitter.instruction("mov rdi, 76");                                 // use PHP's default 76-byte chunk length when omitted
    }
    if inst.operands.len() >= 3 {
        let separator = expect_string_operand(ctx, inst, 2, "chunk_split")?;
        ctx.load_string_value_to_regs(separator, "rax", "rdx")?;
        ctx.emitter.instruction("mov rcx, rax");                                // pass the separator pointer to the runtime helper
        ctx.emitter.instruction("mov r8, rdx");                                 // pass the separator length to the runtime helper
    } else {
        let (label, len) = ctx.data.add_string(b"\r\n");
        abi::emit_symbol_address(ctx.emitter, "rcx", &label);
        abi::emit_load_int_immediate(ctx.emitter, "r8", len as i64);
    }
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Lowers both `strtr()` shapes through their shared runtime helpers.
///
/// The form is selected from the STATIC type of `$from`, not from the operand count: a named
/// `strtr(string: $s, from: [...])` call still materializes the trailing `$to` default, so an
/// array `$from` always means the replacement-pair form. Its container shape then picks
/// between the hash helper and the indexed-array wrapper that converts before replacing.
pub(crate) fn lower_strtr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "strtr", 2, 3)?;
    let pairs = expect_operand(inst, 1)?;
    let helper = match ctx.value_php_type(pairs)? {
        PhpType::AssocArray { .. } => "__rt_strtr_hash",
        PhpType::Array(_) => "__rt_strtr_array",
        _ => return lower_strtr_pairwise(ctx, inst),
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "strtr", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the subject while materializing the replacement pairs
            ctx.load_value_to_result(pairs)?;
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the subject into the primary runtime argument registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "strtr", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.load_value_to_result(pairs)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the replacement pairs to the runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Materializes the three-argument `strtr($string, $from, $to)` byte-translation form.
///
/// A missing or `null` `$to` yields a zero-length destination list, which makes the mapping
/// empty and leaves the subject untouched — the same result php-src produces for
/// `strtr($s, $from, null)` after its deprecation notice.
fn lower_strtr_pairwise(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "strtr", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the subject while materializing the byte lists
            load_string_arg_to_regs(ctx, inst, 1, "strtr", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the source byte list while materializing the destination list
            load_optional_strtr_to(ctx, inst, "x5", "x6")?;
            ctx.emitter.instruction("ldp x3, x4, [sp], #16");                   // restore the source byte list into the runtime argument registers
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the subject into the primary runtime argument registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "strtr", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, "strtr", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_optional_strtr_to(ctx, inst, "rcx", "r8")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_strtr_pairwise");
    store_if_result(ctx, inst)
}

/// Loads the nullable `strtr()` `$to` byte list into a pointer/length pair.
fn load_optional_strtr_to(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(to) = inst.operands.get(2).copied() else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    if matches!(ctx.value_php_type(to)?, PhpType::Void | PhpType::Never) {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    }
    load_value_as_string_to_regs(ctx, to, "strtr to", ptr_reg, len_reg)
}

/// Lowers `count_chars(string, mode?)` through the shared runtime helper.
///
/// The checker already fixed the result storage from the literal `$mode`, so the only runtime
/// validation left is php-src's `ValueError` for a mode outside `0..=4`, raised before
/// `__rt_count_chars` allocates anything.
pub(crate) fn lower_count_chars(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "count_chars", 1, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "count_chars", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the subject while materializing the mode
            if inst.operands.len() >= 2 {
                let mode = expect_operand(inst, 1)?;
                load_as_int(ctx, mode, "count_chars mode")?;
                ctx.emitter.instruction("mov x3, x0");                          // pass the requested result mode to the runtime helper
            } else {
                ctx.emitter.instruction("mov x3, xzr");                         // php's default mode 0 tallies every byte value
            }
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the subject into the primary runtime argument registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "count_chars", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            if inst.operands.len() >= 2 {
                let mode = expect_operand(inst, 1)?;
                load_as_int(ctx, mode, "count_chars mode")?;
                ctx.emitter.instruction("mov rdi, rax");                        // pass the requested result mode to the runtime helper
            } else {
                ctx.emitter.instruction("xor edi, edi");                        // php's default mode 0 tallies every byte value
            }
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    let mode_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedInRange(mode_reg, 0, 4),
        COUNT_CHARS_MODE_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, "__rt_count_chars");
    store_if_result(ctx, inst)
}

/// Lowers the allocation-free `count(count_chars($s, 1))` helper.
pub(crate) fn lower_count_chars_distinct(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "count_chars", 1, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "count_chars", "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_count_chars_distinct");
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "count_chars", "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_count_chars_distinct");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `str_word_count(string, format?, characters?)` through the shared runtime helper.
///
/// The checker already fixed the result storage from the literal `$format`, so the only
/// runtime validation left is php-src's `ValueError` for a format outside `0..=2`, raised
/// before `__rt_str_word_count` allocates anything.
pub(crate) fn lower_str_word_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "str_word_count", 1, 3)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_word_count_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_word_count_x86_64(ctx, inst)?,
    }
    let format_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedInRange(format_reg, 0, 2),
        STR_WORD_COUNT_FORMAT_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, "__rt_str_word_count");
    store_if_result(ctx, inst)
}

/// Materializes AArch64 `str_word_count()` runtime arguments.
fn lower_str_word_count_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "str_word_count")?;
    ctx.load_string_value_to_regs(subject, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject while materializing the format and character list
    if inst.operands.len() >= 2 {
        let format = expect_operand(inst, 1)?;
        load_as_int(ctx, format, "str_word_count format")?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the requested result format to the runtime helper
    } else {
        ctx.emitter.instruction("mov x3, xzr");                                 // php's default format 0 returns the plain word count
    }
    load_optional_str_word_count_characters(ctx, inst, "x4", "x5")?;
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the subject into the primary runtime argument registers
    Ok(())
}

/// Materializes x86_64 `str_word_count()` runtime arguments.
fn lower_str_word_count_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "str_word_count")?;
    ctx.load_string_value_to_regs(subject, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    if inst.operands.len() >= 2 {
        let format = expect_operand(inst, 1)?;
        load_as_int(ctx, format, "str_word_count format")?;
        ctx.emitter.instruction("mov rdi, rax");                                // pass the requested result format to the runtime helper
    } else {
        ctx.emitter.instruction("xor edi, edi");                                // php's default format 0 returns the plain word count
    }
    load_optional_str_word_count_characters(ctx, inst, "rcx", "r8")?;
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Loads the nullable optional `str_word_count()` character list into a pointer/length pair.
///
/// An omitted or `null` `$characters` argument becomes a zero-length list, which builds the
/// same membership table php-src derives from a `NULL` char list.
fn load_optional_str_word_count_characters(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(characters) = inst.operands.get(2).copied() else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    if matches!(ctx.value_php_type(characters)?, PhpType::Void | PhpType::Never) {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    }
    load_value_as_string_to_regs(ctx, characters, "str_word_count characters", ptr_reg, len_reg)
}

/// Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper.
pub(crate) fn lower_str_pad(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "str_pad expected 2 to 4 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_pad_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_pad_x86_64(ctx, inst)?,
    }
    emit_str_pad_argument_guards(ctx, inst.operands.len() >= 4);
    abi::emit_call_label(ctx.emitter, "__rt_str_pad");
    store_if_result(ctx, inst)
}
/// Materializes AArch64 `str_replace`-family runtime arguments.
pub(super) fn lower_string_replace_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, name, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the search string while materializing replacement and subject
    load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the replacement string while materializing the subject
    load_string_arg_to_regs(ctx, inst, 2, name, "x1", "x2")?;
    ctx.emitter.instruction("mov x5, x1");                                      // pass the subject string pointer as the third runtime string argument
    ctx.emitter.instruction("mov x6, x2");                                      // pass the subject string length as the third runtime string argument
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore replacement into the secondary runtime string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore search into the primary runtime string argument
    Ok(())
}

/// Materializes x86_64 `str_replace`-family runtime arguments.
pub(super) fn lower_string_replace_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, name, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 2, name, "rax", "rdx")?;
    ctx.emitter.instruction("mov rcx, rax");                                    // pass the subject string pointer as the third runtime string argument
    ctx.emitter.instruction("mov r8, rdx");                                     // pass the subject string length as the third runtime string argument
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes AArch64 `str_pad()` runtime arguments.
pub(super) fn lower_str_pad_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_operand(inst, 0)?;
    let target_length = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, input, "str_pad", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the input string while materializing length and pad arguments
    load_as_int(ctx, target_length, "str_pad length")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_str_pad_pad_string_aarch64(ctx, inst)?;
    materialize_str_pad_type_aarch64(ctx, inst)?;
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the pad string into secondary runtime argument registers
    abi::emit_pop_reg(ctx.emitter, "x5");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the input string into primary runtime argument registers
    Ok(())
}

/// Materializes the AArch64 `str_pad()` pad-string argument.
pub(super) fn materialize_str_pad_pad_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let pad_string = expect_operand(inst, 2)?;
        load_value_as_string_to_regs(ctx, pad_string, "str_pad", "x1", "x2")?;
    } else {
        let (label, len) = ctx.data.add_string(b" ");
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
    }
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the pad string while materializing the optional pad type
    Ok(())
}

/// Materializes the AArch64 `str_pad()` pad-type argument.
pub(super) fn materialize_str_pad_type_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let pad_type = expect_operand(inst, 3)?;
        load_as_int(ctx, pad_type, "str_pad pad_type")?;
        ctx.emitter.instruction("mov x7, x0");                                  // pass the requested STR_PAD mode to the runtime helper
    } else {
        ctx.emitter.instruction("mov x7, #1");                                  // default to STR_PAD_RIGHT when pad_type is omitted
    }
    Ok(())
}

/// Materializes x86_64 `str_pad()` runtime arguments.
pub(super) fn lower_str_pad_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_operand(inst, 0)?;
    let target_length = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, input, "str_pad", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, target_length, "str_pad length")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_str_pad_pad_string_x86_64(ctx, inst)?;
    materialize_str_pad_type_x86_64(ctx, inst)?;
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the x86_64 `str_pad()` pad-string argument.
pub(super) fn materialize_str_pad_pad_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let pad_string = expect_operand(inst, 2)?;
        load_value_as_string_to_regs(ctx, pad_string, "str_pad", "rax", "rdx")?;
    } else {
        let (label, len) = ctx.data.add_string(b" ");
        abi::emit_symbol_address(ctx.emitter, "rax", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
    }
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the x86_64 `str_pad()` pad-type argument.
pub(super) fn materialize_str_pad_type_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let pad_type = expect_operand(inst, 3)?;
        load_as_int(ctx, pad_type, "str_pad pad_type")?;
        ctx.emitter.instruction("mov r8, rax");                                 // pass the requested STR_PAD mode to the runtime helper
    } else {
        ctx.emitter.instruction("mov r8, 1");                                   // default to STR_PAD_RIGHT when pad_type is omitted
    }
    Ok(())
}

/// Materializes AArch64 `wordwrap()` runtime arguments.
pub(super) fn lower_wordwrap_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, "wordwrap")?;
    ctx.load_string_value_to_regs(input, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the input string while materializing width and break arguments
    materialize_wordwrap_width_aarch64(ctx, inst)?;
    materialize_wordwrap_break_aarch64(ctx, inst)?;
    if inst.operands.len() >= 4 {
        let cut = expect_operand(inst, 3)?;
        load_as_int(ctx, cut, "wordwrap cut")?;
        ctx.emitter.instruction("mov x6, x0");                                  // pass the requested cut_long_words flag to the runtime helper
    } else {
        ctx.emitter.instruction("mov x6, #0");                                  // default cut_long_words to false when omitted
    }
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the input string into primary runtime argument registers
    Ok(())
}

/// Materializes the AArch64 wordwrap width argument.
pub(super) fn materialize_wordwrap_width_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let width = expect_operand(inst, 1)?;
        load_as_int(ctx, width, "wordwrap width")?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the requested wrap width to the runtime helper
    } else {
        ctx.emitter.instruction("mov x3, #75");                                 // use PHP's default wrap width when omitted
    }
    Ok(())
}

/// Materializes the AArch64 wordwrap break-string argument.
pub(super) fn materialize_wordwrap_break_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let break_string = expect_string_operand(ctx, inst, 2, "wordwrap")?;
        ctx.load_string_value_to_regs(break_string, "x1", "x2")?;
        ctx.emitter.instruction("mov x4, x1");                                  // pass the break-string pointer to the runtime helper
        ctx.emitter.instruction("mov x5, x2");                                  // pass the break-string length to the runtime helper
    } else {
        let (label, len) = ctx.data.add_string(b"\n");
        abi::emit_symbol_address(ctx.emitter, "x4", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x5", len as i64);
    }
    Ok(())
}

/// Materializes x86_64 `wordwrap()` runtime arguments.
pub(super) fn lower_wordwrap_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, "wordwrap")?;
    ctx.load_string_value_to_regs(input, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    materialize_wordwrap_width_x86_64(ctx, inst)?;
    materialize_wordwrap_break_x86_64(ctx, inst)?;
    if inst.operands.len() >= 4 {
        let cut = expect_operand(inst, 3)?;
        load_as_int(ctx, cut, "wordwrap cut")?;
        ctx.emitter.instruction("mov r9, rax");                                 // pass the requested cut_long_words flag to the runtime helper
    } else {
        ctx.emitter.instruction("mov r9, 0");                                   // default cut_long_words to false when omitted
    }
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the x86_64 wordwrap width argument.
pub(super) fn materialize_wordwrap_width_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let width = expect_operand(inst, 1)?;
        load_as_int(ctx, width, "wordwrap width")?;
        ctx.emitter.instruction("mov rdi, rax");                                // pass the requested wrap width to the runtime helper
    } else {
        ctx.emitter.instruction("mov rdi, 75");                                 // use PHP's default wrap width when omitted
    }
    Ok(())
}

/// Materializes the x86_64 wordwrap break-string argument.
pub(super) fn materialize_wordwrap_break_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let break_string = expect_string_operand(ctx, inst, 2, "wordwrap")?;
        ctx.load_string_value_to_regs(break_string, "rax", "rdx")?;
        ctx.emitter.instruction("mov rcx, rax");                                // pass the break-string pointer to the runtime helper
        ctx.emitter.instruction("mov r8, rdx");                                 // pass the break-string length to the runtime helper
    } else {
        let (label, len) = ctx.data.add_string(b"\n");
        abi::emit_symbol_address(ctx.emitter, "rcx", &label);
        abi::emit_load_int_immediate(ctx.emitter, "r8", len as i64);
    }
    Ok(())
}
