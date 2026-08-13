//! Purpose:
//! Lowers explode, sscanf, str_split, and implode with temporary string cleanup.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Coercion temporaries are saved and released without disturbing array-result ownership.

use super::*;

use crate::codegen::lower_inst::builtins::arrays::values::emit_loaded_assoc_array_values;

/// Stack cleanup slots for split builtin string coercions that allocate owned temporaries.
pub(super) struct SplitStringTempCleanups {
    delimiter_offset: Option<usize>,
    subject_offset: Option<usize>,
    bytes: usize,
}

impl SplitStringTempCleanups {
    /// Builds a cleanup layout with one 16-byte stack slot for each owned string temporary.
    fn new(delimiter_needs_cleanup: bool, subject_needs_cleanup: bool) -> Self {
        let mut bytes = 0usize;
        let delimiter_offset = delimiter_needs_cleanup.then(|| {
            let offset = bytes;
            bytes += 16;
            offset
        });
        let subject_offset = subject_needs_cleanup.then(|| {
            let offset = bytes;
            bytes += 16;
            offset
        });
        Self {
            delimiter_offset,
            subject_offset,
            bytes,
        }
    }

    /// Returns true when no split string coercion produced an owned temporary.
    fn is_empty(&self) -> bool {
        self.bytes == 0
    }

    /// Returns the stack offsets for all saved owned string temporaries.
    fn offsets(&self) -> impl Iterator<Item = usize> + '_ {
        [self.delimiter_offset, self.subject_offset]
            .into_iter()
            .flatten()
    }
}
/// Lowers `explode(delimiter, string)` into the shared string-array splitter helper.
/// Lowers `dechex()`/`decbin()`/`decoct()` through the shared unsigned base renderer.
///
/// The three builtins differ only in the constant base handed to `__rt_dec_to_base`, which
/// reads its input as unsigned — that is what makes `dechex(-1)` render `"ffffffffffffffff"`
/// instead of a signed value.
pub(crate) fn lower_dec_to_base(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    base: i64,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    load_as_int(ctx, expect_operand(inst, 0)?, name)?;
    let base_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    abi::emit_load_int_immediate(ctx.emitter, base_reg, base);
    abi::emit_call_label(ctx.emitter, "__rt_dec_to_base");
    store_if_result(ctx, inst)
}

/// Lowers `hexdec()`/`bindec()`/`octdec()` through the shared base-digit parser.
///
/// The three builtins differ only in the constant base handed to `__rt_base_to_number`.
/// That helper reports whether its answer stayed an integer or widened to a float, and this
/// lowering boxes the selected arm into the `int|float` union's `Mixed` representation.
pub(crate) fn lower_base_to_number(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    base: i64,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    let subject = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_value_as_string_to_regs(ctx, subject, name, "x1", "x2")?;
            abi::emit_load_int_immediate(ctx.emitter, "x3", base);
        }
        Arch::X86_64 => {
            load_value_as_string_to_regs(ctx, subject, name, "rax", "rdx")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the subject pointer as the first SysV argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the subject length before the base overwrites rdx
            abi::emit_load_int_immediate(ctx.emitter, "rdx", base);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_base_to_number");
    box_base_to_number_result(ctx, name);
    store_if_result(ctx, inst)
}

/// Boxes `__rt_base_to_number`'s integer-or-float answer as PHP's `int|float` union.
///
/// The helper reports its arm in the integer result register: zero selects the integer
/// payload it left alongside it, one selects the float payload in the float result register.
fn box_base_to_number_result(ctx: &mut FunctionContext<'_>, name: &str) {
    let float_label = ctx.next_label(&format!("{}_float", name));
    let done_label = ctx.next_label(&format!("{}_done", name));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", float_label));      // a widened result is boxed from the float register instead
            ctx.emitter.instruction("mov x2, xzr");                             // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip float boxing after producing the integer result
            ctx.emitter.label(&float_label);
            ctx.emitter.instruction("fmov x1, d0");                             // move the widened float bits into the mixed helper payload register
            ctx.emitter.instruction("mov x2, xzr");                             // float mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #2");                              // runtime tag 2 = float
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the parse stay inside PHP's integer range?
            ctx.emitter.instruction(&format!("jnz {}", float_label));           // a widened result is boxed from the float register instead
            ctx.emitter.instruction("mov rdi, rdx");                            // move the parsed integer into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip float boxing after producing the integer result
            ctx.emitter.label(&float_label);
            ctx.emitter.instruction("movq rdi, xmm0");                          // move the widened float bits into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // float mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 2");                              // runtime tag 2 = float
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers `strncmp()`/`strncasecmp()`, which compare only the first `$length` bytes.
///
/// `$length` is screened before the helper runs because reference PHP raises a catchable
/// `ValueError` for a negative value; the runtime helpers therefore treat their bound as
/// unsigned. `name` selects the php-src wording of that diagnostic.
pub(crate) fn lower_length_limited_compare(
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
    let length_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_value_as_string_to_regs(ctx, expect_operand(inst, 0)?, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the first string while materializing the remaining arguments
            load_value_as_string_to_regs(ctx, expect_operand(inst, 1)?, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the second string while materializing the compare length
            load_as_int(ctx, expect_operand(inst, 2)?, name)?;
            ctx.emitter.instruction("mov x5, x0");                              // pass the requested compare length as the fifth runtime argument
            ctx.emitter.instruction("ldp x3, x4, [sp], #16");                   // restore the second string into the secondary runtime string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the first string into the primary runtime string argument
            "x5"
        }
        Arch::X86_64 => {
            load_value_as_string_to_regs(ctx, expect_operand(inst, 0)?, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_value_as_string_to_regs(ctx, expect_operand(inst, 1)?, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_as_int(ctx, expect_operand(inst, 2)?, name)?;
            ctx.emitter.instruction("mov r8, rax");                             // pass the requested compare length as the fifth SysV argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            "r8"
        }
    };
    let message = if name == "strncasecmp" {
        STRNCASECMP_NEGATIVE_LENGTH_MESSAGE
    } else {
        STRNCMP_NEGATIVE_LENGTH_MESSAGE
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(length_reg, 0),
        message,
    );
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `explode(separator, string, limit?)` into the shared string-array splitter helper.
pub(crate) fn lower_explode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let cleanups = plan_split_string_temp_cleanups(ctx, inst)?;
    if !cleanups.is_empty() {
        abi::emit_reserve_temporary_stack(ctx.emitter, cleanups.bytes);
    }
    load_split_pair_args(ctx, inst, "explode", &cleanups)?;
    emit_explode_separator_guard(ctx, &cleanups);
    abi::emit_call_label(ctx.emitter, "__rt_explode");
    emit_split_string_temp_cleanups(ctx, &cleanups);
    store_if_result(ctx, inst)
}

/// Rejects the empty `explode()` separator reference PHP refuses to split on.
///
/// A zero-length separator matches at every position, so the pre-guard splitter advanced its
/// cursor by zero bytes and pushed empty segments until the heap was exhausted. The guard
/// runs after argument materialization, so any owned string temporaries are released on the
/// throwing path before the unwinder takes over.
fn emit_explode_separator_guard(
    ctx: &mut FunctionContext<'_>,
    cleanups: &SplitStringTempCleanups,
) {
    let ok_label = ctx.next_label("explode_separator_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x2, {}", ok_label));         // a non-empty separator can split the subject
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // is the separator zero-length?
            ctx.emitter.instruction(&format!("jnz {}", ok_label));              // a non-empty separator can split the subject
        }
    }
    emit_split_string_temp_cleanups(ctx, cleanups);
    super::super::exceptions::emit_value_error(ctx, EXPLODE_EMPTY_SEPARATOR_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Lowers `sscanf(string, format)` into the shared scanner helper.
pub(crate) fn lower_sscanf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "sscanf expected at least 2 args, got {}",
            inst.operands.len()
        )));
    }
    load_input_and_pattern_args(ctx, inst, "sscanf")?;
    abi::emit_call_label(ctx.emitter, "__rt_sscanf");
    store_if_result(ctx, inst)
}

/// Lowers `str_split(string, length?)` into the fixed-width string-array splitter.
pub(crate) fn lower_str_split(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "str_split expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_split_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_split_x86_64(ctx, inst)?,
    }
    // `__rt_str_split` advances its cursor by the chunk length, so a zero length spins
    // forever pushing empty chunks until the heap is exhausted and a negative one walks
    // the cursor backwards off the string. Reference PHP rejects both up front.
    let length_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(length_reg, 1),
        STR_SPLIT_NON_POSITIVE_LENGTH_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, "__rt_str_split");
    store_if_result(ctx, inst)
}

/// Lowers `implode(glue, array)` / `join(array)` by selecting the array-element helper.
///
/// The typed target is shared by both PHP names, so the operand roles are derived from the
/// argument count rather than the source spelling: a single operand is the ARRAY and the glue
/// is the empty string (`join(["a","b"]) === "ab"`), while two operands keep the ordinary
/// `(glue, array)` order. The reversed PHP 7 order was removed in PHP 8.0 and is not accepted.
pub(crate) fn lower_implode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "implode expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let array_index = inst.operands.len() - 1;
    let runtime_label = implode_runtime_label(ctx, inst, array_index)?;
    let hash_copy = implode_hash_value_type(ctx, inst, array_index)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_implode_aarch64(ctx, inst, array_index)?,
        Arch::X86_64 => lower_implode_x86_64(ctx, inst, array_index)?,
    }
    let Some(value_ty) = hash_copy else {
        abi::emit_call_label(ctx.emitter, runtime_label);
        return store_if_result(ctx, inst);
    };
    // The hash operand was copied into a fresh indexed array the caller owns. It has to
    // outlive the join and then be released, and the join answers in the STRING result
    // register PAIR — so the answer is stacked while the copy is released, rather than the
    // copy being released first, which would free the payload the join just read.
    let array_reg = implode_array_argument_reg(ctx);
    abi::emit_push_reg(ctx.emitter, array_reg);
    abi::emit_call_label(ctx.emitter, runtime_label);
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(value_ty)));
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    store_if_result(ctx, inst)
}

/// The register the implode ABI leaves the array argument in, per target.
fn implode_array_argument_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdx",
    }
}
/// Materializes delimiter/payload string pairs plus the optional `$limit` for `explode()`.
pub(super) fn load_split_pair_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    cleanups: &SplitStringTempCleanups,
) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 or 3 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_split_pair_args_aarch64(ctx, inst, name, cleanups)?,
        Arch::X86_64 => load_split_pair_args_x86_64(ctx, inst, name, cleanups)?,
    }
    load_split_limit_arg(ctx, inst, name)
}

/// Materializes AArch64 delimiter and subject strings for `explode()`.
pub(super) fn load_split_pair_args_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    cleanups: &SplitStringTempCleanups,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, name, "x1", "x2")?;
    if let Some(offset) = cleanups.delimiter_offset {
        save_split_string_temp(ctx, offset, "x1", "x2");
    }
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the delimiter string while materializing the subject string
    load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the subject string pointer as the secondary split argument
    ctx.emitter.instruction("mov x4, x2");                                      // pass the subject string length as the secondary split argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the delimiter string into primary split argument registers
    if let Some(offset) = cleanups.subject_offset {
        save_split_string_temp(ctx, offset, "x3", "x4");
    }
    Ok(())
}

/// Materializes x86_64 delimiter and subject strings for `explode()`.
pub(super) fn load_split_pair_args_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    cleanups: &SplitStringTempCleanups,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, name, "rax", "rdx")?;
    if let Some(offset) = cleanups.delimiter_offset {
        save_split_string_temp(ctx, offset, "rax", "rdx");
    }
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the subject string pointer as the secondary split argument
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the subject string length as the secondary split argument
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    if let Some(offset) = cleanups.subject_offset {
        save_split_string_temp(ctx, offset, "rdi", "rsi");
    }
    Ok(())
}

/// Plans which split builtin operands produce owned temporary strings during coercion.
pub(super) fn plan_split_string_temp_cleanups(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<SplitStringTempCleanups> {
    let delimiter = expect_operand(inst, 0)?;
    let subject = expect_operand(inst, 1)?;
    Ok(SplitStringTempCleanups::new(
        value_string_coercion_needs_temp_cleanup(ctx, delimiter)?,
        value_string_coercion_needs_temp_cleanup(ctx, subject)?,
    ))
}

/// Returns true when string coercion for `value` returns a caller-owned heap string.
pub(super) fn value_string_coercion_needs_temp_cleanup(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(value)?.codegen_repr(),
        PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::TaggedScalar
            | PhpType::Resource(_)
    ))
}

/// Saves a string pointer/length pair into the split builtin cleanup area.
pub(super) fn save_split_string_temp(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    ptr_reg: &str,
    len_reg: &str,
) {
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_temporary_stack_address(ctx.emitter, scratch, offset);
    abi::emit_store_to_address(ctx.emitter, ptr_reg, scratch, 0);
    abi::emit_store_to_address(ctx.emitter, len_reg, scratch, 8);
}

/// Releases owned split string temporaries while preserving the runtime result.
pub(super) fn emit_split_string_temp_cleanups(
    ctx: &mut FunctionContext<'_>,
    cleanups: &SplitStringTempCleanups,
) {
    if cleanups.is_empty() {
        return;
    }
    for offset in cleanups.offsets() {
        let shifted_offset = offset + 16;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            shifted_offset,
        );
        abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    abi::emit_release_temporary_stack(ctx.emitter, cleanups.bytes);
}
/// Materializes primary input and pattern strings for scanner-style helpers.
pub(super) fn load_input_and_pattern_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_input_and_pattern_args_aarch64(ctx, inst, name),
        Arch::X86_64 => load_input_and_pattern_args_x86_64(ctx, inst, name),
    }
}

/// Materializes AArch64 input and pattern strings for `sscanf()`.
pub(super) fn load_input_and_pattern_args_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, name)?;
    let pattern = expect_string_operand(ctx, inst, 1, name)?;
    ctx.load_string_value_to_regs(input, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the scanner input while materializing the pattern string
    ctx.load_string_value_to_regs(pattern, "x1", "x2")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the pattern pointer as the secondary scanner argument
    ctx.emitter.instruction("mov x4, x2");                                      // pass the pattern length as the secondary scanner argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the scanner input into primary argument registers
    Ok(())
}

/// Materializes x86_64 input and pattern strings for `sscanf()`.
pub(super) fn load_input_and_pattern_args_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, name)?;
    let pattern = expect_string_operand(ctx, inst, 1, name)?;
    ctx.load_string_value_to_regs(input, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.load_string_value_to_regs(pattern, "rax", "rdx")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the pattern pointer as the secondary scanner argument
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the pattern length as the secondary scanner argument
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes AArch64 source string and optional chunk length for `str_split()`.
pub(super) fn lower_str_split_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_split")?;
    ctx.load_string_value_to_regs(source, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the chunk length
    materialize_str_split_length_aarch64(ctx, inst)?;
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string before calling the splitter helper
    Ok(())
}

/// Materializes x86_64 source string and optional chunk length for `str_split()`.
pub(super) fn lower_str_split_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_split")?;
    ctx.load_string_value_to_regs(source, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    materialize_str_split_length_x86_64(ctx, inst)?;
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the AArch64 optional `str_split()` chunk length.
pub(super) fn materialize_str_split_length_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let length = expect_operand(inst, 1)?;
        load_as_int(ctx, length, "str_split length")?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the requested chunk length to the splitter helper
    } else {
        ctx.emitter.instruction("mov x3, #1");                                  // default to one-byte chunks when length is omitted
    }
    Ok(())
}

/// Materializes the x86_64 optional `str_split()` chunk length.
pub(super) fn materialize_str_split_length_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let length = expect_operand(inst, 1)?;
        load_as_int(ctx, length, "str_split length")?;
        ctx.emitter.instruction("mov rdi, rax");                                // pass the requested chunk length to the splitter helper
    } else {
        ctx.emitter.instruction("mov rdi, 1");                                  // default to one-byte chunks when length is omitted
    }
    Ok(())
}

/// Returns the runtime helper label required for an `implode()` array operand.
///
/// `array_index` is 1 for the ordinary `(glue, array)` call and 0 for the single-argument
/// `join($array)` form, whose only operand is the array itself.
pub(super) fn implode_runtime_label(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    array_index: usize,
) -> Result<&'static str> {
    let array = expect_operand(inst, array_index)?;
    match ctx.value_php_type(array)? {
        PhpType::Array(elem_ty) => implode_element_runtime_label(&elem_ty),
        // PHP joins an associative array's VALUES and ignores its keys, so the renderer is
        // chosen the same way; the values are copied into an indexed array first.
        PhpType::AssocArray { value, .. } => implode_element_runtime_label(&value),
        PhpType::Mixed | PhpType::Union(_) => Ok("__rt_implode"),
        other => Err(CodegenIrError::unsupported(format!(
            "implode array PHP type {:?}",
            other
        ))),
    }
}

/// Picks the `implode()` renderer for one element type.
fn implode_element_runtime_label(elem_ty: &PhpType) -> Result<&'static str> {
    match elem_ty.codegen_repr() {
        // PHP stringifies bool elements as "1"/"" — NOT as the "1"/"0" that
        // `__rt_implode_int`'s `__rt_itoa` pass would produce — so bool arrays get their
        // own renderer. `PhpType::False` reaches this arm as `Bool` through `codegen_repr`.
        PhpType::Bool => Ok("__rt_implode_bool"),
        PhpType::Int => Ok("__rt_implode_int"),
        // An empty array literal carries an uninhabited element type (`Never`, or
        // `Void` once it has gone through `codegen_repr`). Neither renderer can ever
        // dereference an element, so the generic string helper is the safe choice and
        // keeps `implode("", [])` / `join([])` from being rejected at lowering time.
        PhpType::Str | PhpType::Mixed | PhpType::Never | PhpType::Void => Ok("__rt_implode"),
        other => Err(CodegenIrError::unsupported(format!(
            "implode array element PHP type {:?}",
            other
        ))),
    }
}

/// Returns the associative-array VALUE type when `implode()` must copy its values first.
///
/// The renderers walk a dense indexed payload, so a hash operand is converted through the
/// same extraction `array_values()` uses. That copy is a fresh owned array, which is why the
/// caller has to release it once the join has read it.
fn implode_hash_value_type(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    array_index: usize,
) -> Result<Option<PhpType>> {
    let array = expect_operand(inst, array_index)?;
    match ctx.value_php_type(array)? {
        PhpType::AssocArray { value, .. } => Ok(Some(value.codegen_repr())),
        _ => Ok(None),
    }
}

/// Materializes AArch64 glue and array arguments for `implode()`.
///
/// `array_index` is 0 for the single-argument `join($array)` form, which joins with an empty
/// separator, and 1 for the ordinary `(glue, array)` call.
pub(super) fn lower_implode_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array_index: usize,
) -> Result<()> {
    let array = expect_operand(inst, array_index)?;
    if array_index == 0 {
        let (label, _) = ctx.data.add_string(b"");
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
    } else {
        let glue = expect_operand(inst, 0)?;
        load_value_as_string_to_regs(ctx, glue, "implode", "x1", "x2")?;
    }
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the glue string while materializing the array argument
    load_implode_array_aarch64(ctx, array)?;
    ctx.emitter.instruction("mov x3, x0");                                      // pass the indexed array pointer as the third implode argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the glue string into primary implode argument registers
    Ok(())
}

/// Materializes x86_64 glue and array arguments for `implode()`.
///
/// `array_index` follows the same convention as the AArch64 emitter: 0 selects the
/// single-argument `join($array)` form with an empty separator, 1 the `(glue, array)` call.
pub(super) fn lower_implode_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array_index: usize,
) -> Result<()> {
    let array = expect_operand(inst, array_index)?;
    if array_index == 0 {
        let (label, _) = ctx.data.add_string(b"");
        abi::emit_symbol_address(ctx.emitter, "rax", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", 0);
    } else {
        let glue = expect_operand(inst, 0)?;
        load_value_as_string_to_regs(ctx, glue, "implode", "rax", "rdx")?;
    }
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_implode_array_x86_64(ctx, array)?;
    ctx.emitter.instruction("mov rdx, rax");                                    // pass the indexed array pointer as the third implode argument
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    Ok(())
}

/// Loads the raw indexed-array payload consumed by `implode()` on AArch64.
pub(super) fn load_implode_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed array payload to implode()
            Ok(())
        }
        // A hash has no dense payload for the renderers to walk, so its values are copied
        // into a fresh indexed array first — the same extraction `array_values()` uses.
        // `lower_implode` releases that copy once the join has read it.
        PhpType::AssocArray { value, .. } => {
            ctx.load_value_to_reg(array, "x0")?;
            emit_loaded_assoc_array_values(ctx, &value.codegen_repr())
        }
        _ => {
            ctx.load_value_to_reg(array, "x0")?;
            Ok(())
        }
    }
}

/// Loads the raw indexed-array payload consumed by `implode()` on x86_64.
pub(super) fn load_implode_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov rax, rdi");                            // pass the unboxed array payload to implode()
            Ok(())
        }
        // See the AArch64 loader: a hash operand is copied into an indexed array first.
        PhpType::AssocArray { value, .. } => {
            ctx.load_value_to_reg(array, "rax")?;
            emit_loaded_assoc_array_values(ctx, &value.codegen_repr())
        }
        _ => {
            ctx.load_value_to_reg(array, "rax")?;
            Ok(())
        }
    }
}
