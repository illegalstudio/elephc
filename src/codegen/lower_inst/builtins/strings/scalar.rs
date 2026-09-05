//! Purpose:
//! Lowers scalar/string conversions and `number_format` argument handling.
//!
//! Called from:
//! - The string builtin lowering facade and first-character case helpers.
//!
//! Key details:
//! - Numeric coercions and separator defaults remain PHP-compatible across targets.

use super::*;

const FLOAT_TO_INT_BITS_OFFSET: usize = 0;
const FLOAT_TO_INT_VALUE_OFFSET: usize = 8;
const FLOAT_TO_INT_FRAME_BYTES: usize = 16;
const MIXED_TO_INT_CELL_OFFSET: usize = 0;
const MIXED_TO_INT_FRAME_BYTES: usize = 16;

/// Lowers `ord()` by returning the first byte of a string or zero for empty input.
pub(crate) fn lower_ord(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ord")?;
    let empty_label = ctx.next_label("ord_empty");
    let done_label = ctx.next_label("ord_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {}", empty_label));       // return zero when ord() receives an empty string
            ctx.emitter.instruction("ldrb w0, [x1]");                           // load the first byte as an unsigned integer
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after loading the first byte
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // return zero when ord() receives an empty string
            ctx.emitter.instruction(&format!("jz {}", empty_label));            // branch to the empty-string fallback when the length is zero
            ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");               // load the first byte as an unsigned integer
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after loading the first byte
        }
    }
    ctx.emitter.label(&empty_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `chr()` by converting an integer code point into a one-byte string.
pub(crate) fn lower_chr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "chr expected 1 arg, got {}",
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    load_as_int(ctx, value, "chr")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the character code to the x86_64 runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_chr");
    store_if_result(ctx, inst)
}

/// Lowers `number_format()` by arranging its runtime helper arguments.
pub(crate) fn lower_number_format(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "number_format expected 1 to 4 args, got {}",
            inst.operands.len()
        )));
    }

    let number = expect_operand(inst, 0)?;
    load_as_float(ctx, number, "number_format")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));

    push_decimal_count(ctx, inst)?;
    push_separator_byte(ctx, inst, 2, 46, false, "decimal separator")?;
    push_separator_byte(ctx, inst, 3, 44, true, "thousands separator")?;
    pop_number_format_args(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_number_format");
    store_if_result(ctx, inst)
}
/// Pushes the explicit or default decimal-count argument.
pub(super) fn push_decimal_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() >= 2 {
        let decimals = expect_operand(inst, 1)?;
        load_as_int(ctx, decimals, "number_format decimals")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Pushes a one-byte separator argument, using `default_byte` when it is omitted.
pub(super) fn push_separator_byte(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operand_index: usize,
    default_byte: i64,
    empty_string_means_zero: bool,
    name: &str,
) -> Result<()> {
    if inst.operands.len() > operand_index {
        let value = expect_operand(inst, operand_index)?;
        load_separator_byte(ctx, value, empty_string_means_zero, name)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), default_byte);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Loads the first byte of a separator string into the integer result register.
pub(super) fn load_separator_byte(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    empty_string_means_zero: bool,
    name: &str,
) -> Result<()> {
    if ctx.value_php_type(value)? != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "number_format {} for non-string operand",
            name
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            if empty_string_means_zero {
                emit_aarch64_empty_separator_guard(ctx);
            } else {
                ctx.emitter.instruction("ldrb w0, [x1]");                       // load the first byte of the separator string
            }
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "rax", "rdx")?;
            if empty_string_means_zero {
                emit_x86_64_empty_separator_guard(ctx);
            } else {
                ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");           // load the first byte of the separator string
            }
        }
    }
    Ok(())
}

/// Emits the AArch64 empty-string fallback for the optional thousands separator.
pub(super) fn emit_aarch64_empty_separator_guard(ctx: &mut FunctionContext<'_>) {
    let use_zero = ctx.next_label("nf_sep_zero");
    let done = ctx.next_label("nf_sep_done");
    ctx.emitter.instruction(&format!("cbz x2, {}", use_zero));                  // use the no-separator sentinel when the separator string is empty
    ctx.emitter.instruction("ldrb w0, [x1]");                                   // load the first byte of the non-empty separator string
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the empty-string separator fallback
    ctx.emitter.label(&use_zero);
    abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
    ctx.emitter.label(&done);
}

/// Emits the x86_64 empty-string fallback for the optional thousands separator.
pub(super) fn emit_x86_64_empty_separator_guard(ctx: &mut FunctionContext<'_>) {
    let use_zero = ctx.next_label("nf_sep_zero");
    let done = ctx.next_label("nf_sep_done");
    ctx.emitter.instruction("test rdx, rdx");                                   // check whether the separator string is empty
    ctx.emitter.instruction(&format!("jz {}", use_zero));                       // use the no-separator sentinel for an empty separator
    ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");                       // load the first byte of the non-empty separator string
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the empty-string separator fallback
    ctx.emitter.label(&use_zero);
    abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    ctx.emitter.label(&done);
}

/// Pops the staged arguments into the runtime helper's target ABI registers.
pub(super) fn pop_number_format_args(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_float_reg(ctx.emitter, "d0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_pop_float_reg(ctx.emitter, "xmm0");
        }
    }
}

/// Loads a concrete scalar value as a floating-point runtime argument.
pub(super) fn load_as_float(ctx: &mut FunctionContext<'_>, value: ValueId, name: &str) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Float => Ok(()),
        PhpType::Int | PhpType::Bool => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Converts a weak builtin float argument to int with PHP's diagnostics.
pub(crate) fn emit_weak_float_result_to_int(
    ctx: &mut FunctionContext<'_>,
    context: &str,
) {
    let invalid = ctx.next_label("weak_float_to_int_invalid");
    let exact = ctx.next_label("weak_float_to_int_exact");
    let done = ctx.next_label("weak_float_to_int_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, FLOAT_TO_INT_FRAME_BYTES);
    save_float_result_bits(ctx);
    super::super::super::mixed_narrowing::emit_float_result_fits_i64_or_jump(ctx, &invalid);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        FLOAT_TO_INT_VALUE_OFFSET,
    );
    restore_float_result_bits(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", FLOAT_TO_INT_VALUE_OFFSET);
            ctx.emitter.instruction("scvtf d1, x10");                           // reconstruct the truncated value for an exactness check
            ctx.emitter.instruction("fcmp d0, d1");                             // detect fractional precision loss
            ctx.emitter.instruction(&format!("b.eq {exact}"));                  // integral floats require no deprecation
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", FLOAT_TO_INT_VALUE_OFFSET);
            ctx.emitter.instruction("cvtsi2sd xmm1, r11");                      // reconstruct the truncated value for an exactness check
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // detect fractional precision loss
            ctx.emitter.instruction(&format!("jp {invalid}"));                  // keep unordered values out of the equality branch
            ctx.emitter.instruction(&format!("je {exact}"));                    // integral floats require no deprecation
        }
    }
    emit_float_precision_deprecation(ctx);
    ctx.emitter.label(&exact);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        FLOAT_TO_INT_VALUE_OFFSET,
    );
    abi::emit_release_temporary_stack(ctx.emitter, FLOAT_TO_INT_FRAME_BYTES);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&invalid);
    abi::emit_release_temporary_stack(ctx.emitter, FLOAT_TO_INT_FRAME_BYTES);
    super::super::super::exceptions::emit_type_error(
        ctx,
        &weak_float_type_error_message(context),
    );
    ctx.emitter.label(&done);
}

/// Converts an explicit float cast while warning for values outside the PHP int range.
pub(crate) fn emit_explicit_float_result_to_int(ctx: &mut FunctionContext<'_>) {
    emit_nonweak_float_result_to_int(ctx, false, false);
}

/// Converts a signal-set float with PHP's cast and precision diagnostics.
pub(crate) fn emit_signal_float_result_to_int(ctx: &mut FunctionContext<'_>) {
    emit_nonweak_float_result_to_int(ctx, true, true);
}

/// Converts a non-weak float, selecting whether fractional and NaN precision diagnostics apply.
fn emit_nonweak_float_result_to_int(
    ctx: &mut FunctionContext<'_>,
    deprecate_fraction: bool,
    deprecate_nan: bool,
) {
    let invalid = ctx.next_label("cast_float_to_int_invalid");
    let exact = ctx.next_label("cast_float_to_int_exact");
    let invalid_done = ctx.next_label("cast_float_to_int_invalid_done");
    let done = ctx.next_label("cast_float_to_int_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, FLOAT_TO_INT_FRAME_BYTES);
    save_float_result_bits(ctx);
    super::super::super::mixed_narrowing::emit_float_result_fits_i64_or_jump(ctx, &invalid);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        FLOAT_TO_INT_VALUE_OFFSET,
    );
    if deprecate_fraction {
        restore_float_result_bits(ctx);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", FLOAT_TO_INT_VALUE_OFFSET);
                ctx.emitter.instruction("scvtf d1, x10");                       // reconstruct the truncated signal for an exactness check
                ctx.emitter.instruction("fcmp d0, d1");                         // detect fractional precision loss
                ctx.emitter.instruction(&format!("b.eq {exact}"));              // integral finite values need no deprecation
            }
            Arch::X86_64 => {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", FLOAT_TO_INT_VALUE_OFFSET);
                ctx.emitter.instruction("cvtsi2sd xmm1, r11");                  // reconstruct the truncated signal for an exactness check
                ctx.emitter.instruction("ucomisd xmm0, xmm1");                  // detect fractional precision loss
                ctx.emitter.instruction(&format!("jp {invalid}"));              // keep NaN on the non-representable path
                ctx.emitter.instruction(&format!("je {exact}"));                // integral finite values need no deprecation
            }
        }
        emit_float_precision_deprecation(ctx);
    }
    ctx.emitter.label(&exact);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        FLOAT_TO_INT_VALUE_OFFSET,
    );
    abi::emit_release_temporary_stack(ctx.emitter, FLOAT_TO_INT_FRAME_BYTES);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&invalid);
    emit_float_not_representable_warning(ctx);
    if deprecate_nan {
        restore_float_result_bits(ctx);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fcmp d0, d0");                         // only NaN receives the additional precision deprecation
                ctx.emitter.instruction(&format!("b.vc {invalid_done}"));       // ordered infinities and extremes skip it
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("ucomisd xmm0, xmm0");                  // only NaN receives the additional precision deprecation
                ctx.emitter.instruction(&format!("jnp {invalid_done}"));        // ordered infinities and extremes skip it
            }
        }
        emit_float_precision_deprecation(ctx);
    }
    ctx.emitter.label(&invalid_done);
    restore_float_result_bits(ctx);
    abi::emit_float_result_to_int_result(ctx.emitter);
    abi::emit_release_temporary_stack(ctx.emitter, FLOAT_TO_INT_FRAME_BYTES);
    ctx.emitter.label(&done);
}

/// Saves the exact float result bits in the conversion scratch frame.
fn save_float_result_bits(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov x9, d0");                             // preserve the exact float bits across diagnostics
            abi::emit_store_to_sp(ctx.emitter, "x9", FLOAT_TO_INT_BITS_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq r10, xmm0");                          // preserve the exact float bits across diagnostics
            abi::emit_store_to_sp(ctx.emitter, "r10", FLOAT_TO_INT_BITS_OFFSET);
        }
    }
}

/// Restores the exact saved float bits into the target float result register.
fn restore_float_result_bits(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", FLOAT_TO_INT_BITS_OFFSET);
            ctx.emitter.instruction("fmov d0, x9");                             // restore the original float payload
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", FLOAT_TO_INT_BITS_OFFSET);
            ctx.emitter.instruction("movq xmm0, r10");                          // restore the original float payload
        }
    }
}

/// Emits PHP's shortest-round-trip precision-loss deprecation for the saved float.
fn emit_float_precision_deprecation(ctx: &mut FunctionContext<'_>) {
    emit_static_int_coercion_diagnostic(ctx, "Deprecated: Implicit conversion from float ");
    emit_saved_float_diagnostic_value(ctx);
    emit_static_int_coercion_diagnostic(ctx, " to int loses precision\n");
}

/// Emits PHP's warning for an explicit float value outside the integer range.
fn emit_float_not_representable_warning(ctx: &mut FunctionContext<'_>) {
    emit_static_int_coercion_diagnostic(ctx, "Warning: The float ");
    emit_saved_float_diagnostic_value(ctx);
    emit_static_int_coercion_diagnostic(ctx, " is not representable as an int, cast occurred\n");
}

/// Formats the saved float with `__rt_ftoa_repr` and emits it as a diagnostic fragment.
fn emit_saved_float_diagnostic_value(ctx: &mut FunctionContext<'_>) {
    restore_float_result_bits(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_ftoa_repr");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the formatted float pointer to the diagnostic helper
        ctx.emitter.instruction("mov rsi, rdx");                                // pass the formatted float length to the diagnostic helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Builds the exact weak builtin argument TypeError from the shared builtin contract.
fn weak_float_type_error_message(context: &str) -> String {
    let mut words = context.split_whitespace();
    let function_name = words.next().unwrap_or(context);
    let hints = words.collect::<Vec<_>>();
    if let Some(def) = crate::builtins::registry::lookup(function_name) {
        let parameter = def
            .params
            .iter()
            .enumerate()
            .find(|(_, (name, _))| hints.iter().any(|hint| hint == name))
            .or_else(|| {
                def.params
                    .iter()
                    .enumerate()
                    .find(|(_, (_, ty))| php_type_accepts_int(ty))
            });
        if let Some((index, (parameter_name, _))) = parameter {
            return format!(
                "{}(): Argument #{} (${}) must be of type int, float given",
                def.name,
                index + 1,
                parameter_name
            );
        }
    }
    format!("{function_name}(): Argument must be of type int, float given")
}

/// Returns whether a builtin contract type accepts an integer argument.
fn php_type_accepts_int(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int => true,
        PhpType::Union(members) => members.iter().any(php_type_accepts_int),
        _ => false,
    }
}

/// Casts a boxed weak integer argument while preserving float precision diagnostics.
fn emit_mixed_weak_int(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    let from_float = ctx.next_label("weak_mixed_to_int_float");
    let done = ctx.next_label("weak_mixed_to_int_done");
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, MIXED_TO_INT_FRAME_BYTES);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        MIXED_TO_INT_CELL_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // tag 2 is the only scalar cast that can lose integer precision
            ctx.emitter.instruction(&format!("b.eq {from_float}"));             // route floats through the deprecating weak conversion
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", MIXED_TO_INT_CELL_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // tag 2 is the only scalar cast that can lose integer precision
            ctx.emitter.instruction(&format!("je {from_float}"));               // route floats through the deprecating weak conversion
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", MIXED_TO_INT_CELL_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_TO_INT_FRAME_BYTES);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_float);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("fmov d0, x1"),                // move the unboxed float payload into the shared FP result register
        Arch::X86_64 => ctx.emitter.instruction("movq xmm0, rdi"),              // move the unboxed float payload into the shared FP result register
    }
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_TO_INT_FRAME_BYTES);
    emit_weak_float_result_to_int(ctx, context);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits one suppressible static fragment of an implicit integer-coercion diagnostic.
fn emit_static_int_coercion_diagnostic(ctx: &mut FunctionContext<'_>, message: &str) {
    let (label, len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Loads a concrete scalar value as an integer runtime argument.
pub(crate) fn load_as_int(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            emit_weak_float_result_to_int(ctx, name);
            Ok(())
        }
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_weak_int(ctx, value, name),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Loads a concrete scalar as an explicit integer conversion without precision deprecations.
pub(crate) fn load_as_explicit_int(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            emit_explicit_float_result_to_int(ctx);
            Ok(())
        }
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
