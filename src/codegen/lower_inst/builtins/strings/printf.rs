//! Purpose:
//! Lowers sprintf/printf and array-backed variants with typed variadic packing.
//!
//! Called from:
//! - The string builtin facade and `fprintf` IO lowering.
//!
//! Key details:
//! - Format categories drive 16-byte runtime records without changing source evaluation order.

use super::*;

/// Runtime payload category consumed by one printf-family conversion specifier.
#[derive(Clone, Copy)]
pub(in crate::codegen::lower_inst::builtins) enum SprintfSpecCat {
    /// Integer-like printf specifiers such as `%d`, `%x`, and the runtime default.
    Int,
    /// Floating-point printf specifiers such as `%f`, `%e`, and `%g`.
    Float,
    /// String printf specifier `%s`.
    Str,
}
/// Lowers `sprintf(format, values...)` by packing variadic records for `__rt_sprintf`.
pub(crate) fn lower_sprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_sprintf_runtime_call(ctx, inst, "sprintf")?;
    store_if_result(ctx, inst)
}

/// Lowers `printf(format, values...)` as `sprintf()` followed by stdout emission.
pub(crate) fn lower_printf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_sprintf_runtime_call(ctx, inst, "printf")?;
    emit_printf_write_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `vsprintf(format, values)` through the array-to-sprintf runtime bridge.
pub(crate) fn lower_vsprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_vsprintf_runtime_call(ctx, inst, "vsprintf")?;
    store_if_result(ctx, inst)
}

/// Lowers `vprintf(format, values)` as `vsprintf()` followed by stdout emission.
pub(crate) fn lower_vprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_vsprintf_runtime_call(ctx, inst, "vprintf")?;
    emit_printf_write_result(ctx);
    store_if_result(ctx, inst)
}

/// Packs sprintf-style operands and calls the shared `__rt_sprintf` formatter.
pub(super) fn emit_sprintf_runtime_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.is_empty() {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected at least 1 arg",
            name
        )));
    }
    let format = expect_operand(inst, 0)?;
    let spec_cats = sprintf_spec_cats_for_format(ctx, format)?;
    for index in (1..inst.operands.len()).rev() {
        let value = expect_operand(inst, index)?;
        let spec_cat = spec_cats.get(index - 1).copied();
        pack_sprintf_like_arg(ctx, value, spec_cat, name)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_value_as_string_to_regs(ctx, format, name, "x1", "x2")?;
            ctx.emitter.instruction(&format!("mov x0, #{}", inst.operands.len() - 1)); // pass the number of packed sprintf() variadic records
        }
        Arch::X86_64 => {
            load_value_as_string_to_regs(ctx, format, name, "rax", "rdx")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdi", (inst.operands.len() - 1) as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_sprintf");
    Ok(())
}

/// Returns printf-family specifier categories for a literal format value.
pub(in crate::codegen::lower_inst::builtins) fn sprintf_spec_cats_for_format(
    ctx: &FunctionContext<'_>,
    format: ValueId,
) -> Result<Vec<SprintfSpecCat>> {
    let Some(value_ref) = ctx.function.value(format) else {
        return Err(CodegenIrError::missing_entry("value", format.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(Vec::new());
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let (Op::ConstStr, Some(Immediate::Data(data))) = (inst_ref.op, inst_ref.immediate.as_ref()) else {
        return Ok(Vec::new());
    };
    let raw = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let bytes = crate::string_bytes::literal_bytes(raw);
    Ok(parse_sprintf_spec_cats(&bytes))
}

/// Parses the conversion categories consumed by the runtime sprintf scanner.
/// Highest `printf`-family argument position `parse_sprintf_spec_cats` will track. A format
/// string is program text, so its `N$` digits are attacker-controlled; the cap keeps the
/// category table from being sized by them. Positions above it fall back to static-type
/// packing and are rejected by the runtime's argument-count check.
const MAX_TRACKED_SPRINTF_ARGS: usize = 4096;



















/// Parses the conversion categories consumed by the runtime sprintf scanner, indexed by the
/// argument position each conversion consumes.
///
/// The result must agree with `__rt_sprintf`'s own specifier parser, because the runtime
/// dispatches on the conversion character while this pass decides how the operand is
/// coerced and tagged. That means recognizing everything the runtime recognizes: PHP's
/// `N$` explicit argument numbers (which select a position without advancing the sequential
/// cursor, exactly like PHP), the `'X` custom-pad-character flag (whose `X` must not be
/// mistaken for the conversion character), and the full float conversion set
/// `f F e E g G`. Positions no conversion refers to keep an inert `Str` coercion; the
/// runtime never reads those records.
pub(super) fn parse_sprintf_spec_cats(format: &[u8]) -> Vec<SprintfSpecCat> {
    let mut cats: Vec<Option<SprintfSpecCat>> = Vec::new();
    let mut next_arg = 0usize;
    let mut index = 0;
    while index < format.len() {
        if format[index] != b'%' {
            index += 1;
            continue;
        }
        index += 1;
        if index >= format.len() {
            break;
        }
        if format[index] == b'%' {
            index += 1;
            continue;
        }
        let mut explicit: Option<usize> = None;
        let mut probe = index;
        while probe < format.len() && format[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > index && probe < format.len() && format[probe] == b'$' {
            let mut value: usize = 0;
            for digit in &format[index..probe] {
                value = value
                    .saturating_mul(10)
                    .saturating_add((digit - b'0') as usize);
            }
            explicit = Some(value);
            index = probe + 1;
        }
        while index < format.len() {
            match format[index] {
                b'-' | b'+' | b'0' | b' ' | b'#' => index += 1,
                b'\'' => index += 2,
                _ => break,
            }
        }
        while index < format.len() && format[index].is_ascii_digit() {
            index += 1;
        }
        if index < format.len() && format[index] == b'.' {
            index += 1;
            while index < format.len() && format[index].is_ascii_digit() {
                index += 1;
            }
        }
        if index >= format.len() {
            break;
        }
        let cat = match format[index] {
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => SprintfSpecCat::Float,
            b's' => SprintfSpecCat::Str,
            _ => SprintfSpecCat::Int,
        };
        index += 1;
        let slot = match explicit {
            // `%0$s` has no operand, and an argument number far past any real call cannot
            // match one either. Both are left for the runtime's argument-count check rather
            // than sizing this table from an attacker-controlled digit run.
            Some(0) => continue,
            Some(number) if number > MAX_TRACKED_SPRINTF_ARGS => continue,
            Some(number) => number - 1,
            None => {
                let slot = next_arg;
                next_arg += 1;
                slot
            }
        };
        if slot >= cats.len() {
            cats.resize(slot + 1, None);
        }
        cats[slot] = Some(cat);
    }
    cats.into_iter()
        .map(|cat| cat.unwrap_or(SprintfSpecCat::Str))
        .collect()
}

/// Preserves the format string, evaluates the values array, and calls `__rt_vsprintf`.
pub(super) fn emit_vsprintf_runtime_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected exactly 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    let format = expect_operand(inst, 0)?;
    let values = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(format, "x1", "x2")?;
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve scratch storage for the format string
            ctx.emitter.instruction("stp x1, x2, [sp, #0]");                    // save the format pointer and length across array evaluation
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("ldp x1, x2, [sp, #0]");                    // restore the format pointer and length for vsprintf
            ctx.emitter.instruction("add sp, sp, #16");                         // release the format scratch storage
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(format, "rax", "rdx")?;
            ctx.emitter.instruction("sub rsp, 16");                             // reserve scratch storage for the format string
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the format pointer across array evaluation
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // save the format byte length across array evaluation
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the values array pointer to vsprintf
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // restore the format pointer for vsprintf
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the format byte length for vsprintf
            ctx.emitter.instruction("add rsp, 16");                             // release the format scratch storage
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
    Ok(())
}
/// Packs one printf-family variadic operand into the runtime's 16-byte tagged record.
pub(in crate::codegen::lower_inst::builtins) fn pack_sprintf_like_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    spec_cat: Option<SprintfSpecCat>,
    owner: &str,
) -> Result<()> {
    match spec_cat {
        Some(SprintfSpecCat::Int) => {
            load_sprintf_arg_as_int(ctx, value, owner)?;
            pack_sprintf_int_arg(ctx)
        }
        Some(SprintfSpecCat::Float) => {
            load_sprintf_arg_as_float(ctx, value, owner)?;
            pack_sprintf_float_arg(ctx)
        }
        Some(SprintfSpecCat::Str) => {
            load_sprintf_arg_as_string(ctx, value, owner)?;
            pack_sprintf_string_arg(ctx)
        }
        None => pack_static_sprintf_arg(ctx, value, owner),
    }
}

/// Packs one sprintf variadic operand using its static PHP representation.
///
/// A Mixed operand cannot be packed from its static type — there is nothing static about it — so
/// it goes through `__rt_sprintf_pack_mixed`, which reads the cell's real runtime tag. This
/// matters only when the format string is NOT a compile-time literal: with a literal format,
/// `sprintf_spec_cats_for_format` knows each conversion category and the caller takes the
/// `load_sprintf_arg_as_*` path instead, which already handled Mixed.
///
/// Both conditions together are what made `sprintf($fmt, $v)` with both read out of a
/// heterogeneous array answer `0`: no known category, so the operand fell to the catch-all arm
/// below, which pushes a zero payload tagged as an integer for every shape it does not name.
/// `echo` printed the same value correctly, which is why this read as a formatting bug rather
/// than a marshalling one.
pub(super) fn pack_static_sprintf_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<()> {
    if matches!(
        ctx.raw_value_php_type(value)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        load_value_to_first_int_arg(ctx, value)?;
        abi::emit_call_label(ctx.emitter, "__rt_sprintf_pack_mixed");
        return pack_sprintf_prepacked_arg(ctx);
    }
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &ty, owner),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &ty, owner),
    }
}

/// Pushes a 16-byte record whose payload and tag words a helper already built.
///
/// `__rt_sprintf_pack_mixed` returns the pair in the same registers the int|false stat helpers
/// use — payload in `x0`/`rax`, metadata in `x1`/`rdx` — so this only has to store them.
fn pack_sprintf_prepacked_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // push the packed sprintf operand payload
            ctx.emitter.instruction("str x1, [sp, #8]");                        // store the packed tag/length metadata word
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the packed sprintf operand payload
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // store the packed tag/length metadata word
        }
    }
    Ok(())
}

/// Loads an operand as the integer payload consumed by integer printf specifiers.
pub(super) fn load_sprintf_arg_as_int(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    match raw_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_float_result_to_int_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} integer format argument PHP type {:?}",
                owner, other
            )))
        }
    }
    Ok(())
}

/// Loads an operand as the floating payload consumed by float printf specifiers.
pub(super) fn load_sprintf_arg_as_float(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    match raw_ty.codegen_repr() {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} float format argument PHP type {:?}",
                owner, other
            )))
        }
    }
    Ok(())
}

/// Loads an operand as the pointer/length payload consumed by string printf specifiers.
pub(super) fn load_sprintf_arg_as_string(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_value_as_string_to_regs(ctx, value, owner, "x1", "x2"),
        Arch::X86_64 => load_value_as_string_to_regs(ctx, value, owner, "rax", "rdx"),
    }
}

/// Packs the loaded integer result as a printf-family record.
pub(super) fn pack_sprintf_int_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &PhpType::Int, "sprintf"),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &PhpType::Int, "sprintf"),
    }
}

/// Packs the loaded floating result as a printf-family record.
pub(super) fn pack_sprintf_float_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &PhpType::Float, "sprintf"),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &PhpType::Float, "sprintf"),
    }
}

/// Packs the loaded string result as a printf-family record.
pub(super) fn pack_sprintf_string_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &PhpType::Str, "sprintf"),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &PhpType::Str, "sprintf"),
    }
}

/// Packs one AArch64 sprintf operand from result registers into `[value, tag]`.
pub(super) fn pack_sprintf_arg_aarch64(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
    owner: &str,
) -> Result<()> {
    match ty {
        PhpType::Int => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // push the integer sprintf operand payload
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // tag this sprintf operand record as integer
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov x0, d0");                             // move the float bits into an integer register for packing
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // push the floating sprintf operand payload bits
            ctx.emitter.instruction("mov x0, #2");                              // select runtime sprintf type tag 2 for floats
            ctx.emitter.instruction("str x0, [sp, #8]");                        // store the floating sprintf operand tag
        }
        PhpType::Bool => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // push the boolean sprintf operand payload
            ctx.emitter.instruction("mov x0, #3");                              // select runtime sprintf type tag 3 for bools
            ctx.emitter.instruction("str x0, [sp, #8]");                        // store the boolean sprintf operand tag
        }
        PhpType::Str => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // push the string pointer sprintf operand payload
            ctx.emitter.instruction("lsl x0, x2, #8");                          // shift the string length into the packed metadata word
            ctx.emitter.instruction("orr x0, x0, #1");                          // mark the sprintf operand metadata as a string
            ctx.emitter.instruction("str x0, [sp, #8]");                        // store the packed string length and type tag
        }
        PhpType::Void | PhpType::Never => {
            // A statically known null is a ZERO-LENGTH STRING record, not an integer zero. That
            // is what makes `%s` render "" and `%d` render 0, matching PHP on both — the
            // formatter guards a null string pointer on every conversion path. Tagged as an
            // integer instead, `sprintf($fmt, null)` printed "0" under `%s`.
            ctx.emitter.instruction("str xzr, [sp, #-16]!");                    // null string pointer payload
            ctx.emitter.instruction("mov x0, #1");                              // (0 << 8) | 1 = a zero-length string record
            ctx.emitter.instruction("str x0, [sp, #8]");                        // store the packed string length and type tag
        }
        _other => {
            ctx.emitter.instruction("str xzr, [sp, #-16]!");                    // push a zero payload for an unsupported sprintf operand
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // tag the unsupported sprintf operand as integer zero
        }
    }
    let _ = owner;
    Ok(())
}

/// Packs one x86_64 sprintf operand from result registers into `[value, tag]`.
pub(super) fn pack_sprintf_arg_x86_64(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
    owner: &str,
) -> Result<()> {
    match ty {
        PhpType::Int => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the integer sprintf operand payload
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // tag this sprintf operand record as integer
        }
        PhpType::Float => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("movsd QWORD PTR [rsp], xmm0");             // store the floating sprintf operand payload bits
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 2");              // tag this sprintf operand record as float
        }
        PhpType::Bool => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the boolean sprintf operand payload
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 3");              // tag this sprintf operand record as bool
        }
        PhpType::Str => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the string pointer sprintf operand payload
            ctx.emitter.instruction("mov rcx, rdx");                            // copy the string length before packing metadata
            ctx.emitter.instruction("shl rcx, 8");                              // shift the string length into the packed metadata word
            ctx.emitter.instruction("or rcx, 1");                               // mark the sprintf operand metadata as a string
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rcx");            // store the packed string length and type tag
        }
        PhpType::Void | PhpType::Never => {
            // See the AArch64 half: a statically known null packs as a zero-length STRING record
            // so that `%s` renders "" and `%d` renders 0, both matching PHP.
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                  // null string pointer payload
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 1");              // (0 << 8) | 1 = a zero-length string record
        }
        _other => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve one packed sprintf operand record
            ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                  // store a zero payload for an unsupported sprintf operand
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // tag the unsupported sprintf operand as integer zero
        }
    }
    let _ = owner;
    Ok(())
}

/// Writes the formatted string result to stdout and leaves printf's byte count in the int result register.
pub(super) fn emit_printf_write_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("stp x2, xzr, [sp, #-16]!");                // preserve the formatted byte count across the funnel call
            ctx.emitter.instruction("mov x0, x1");                              // pass the formatted string pointer as the write buffer
            ctx.emitter.instruction("mov x1, x2");                              // pass the formatted string length as the write byte count
            abi::emit_call_label(ctx.emitter, "__rt_stdout_write");
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // return the formatted byte count as printf()'s integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rdx");                                // preserve the formatted byte count across the funnel call
            ctx.emitter.instruction("push rdx");                                // keep the stack 16-byte aligned for the call
            ctx.emitter.instruction("mov rdi, rax");                            // pass the formatted string pointer as the write buffer
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the formatted string length as the write byte count
            abi::emit_call_label(ctx.emitter, "__rt_stdout_write");
            ctx.emitter.instruction("pop rax");                                 // drop the alignment copy of the byte count
            ctx.emitter.instruction("pop rax");                                 // return the formatted byte count as printf()'s integer result
        }
    }
}
