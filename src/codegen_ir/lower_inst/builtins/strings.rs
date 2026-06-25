//! Purpose:
//! Lowers string-returning scalar builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Runtime helpers keep owning returned string storage; this module only
//!   materializes target ABI arguments from EIR SSA slots.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::predicates;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Stack cleanup slots for split builtin string coercions that allocate owned temporaries.
struct SplitStringTempCleanups {
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

/// Runtime payload category consumed by one printf-family conversion specifier.
#[derive(Clone, Copy)]
pub(super) enum SprintfSpecCat {
    /// Integer-like printf specifiers such as `%d`, `%x`, and the runtime default.
    Int,
    /// Floating-point printf specifiers such as `%f`, `%e`, and `%g`.
    Float,
    /// String printf specifier `%s`.
    Str,
}

/// Lowers a one-argument string builtin that directly delegates to a runtime helper.
pub(super) fn lower_unary_string_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_single_string_arg(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `grapheme_strrev()` and boxes its `string|false` result as `Mixed`.
pub(super) fn lower_grapheme_strrev(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "grapheme_strrev")?;
    abi::emit_call_label(ctx.emitter, "__rt_grapheme_strrev");
    box_grapheme_strrev_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `ucfirst()` by copying the string and uppercasing the first ASCII byte.
pub(super) fn lower_ucfirst(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ucfirst")?;
    abi::emit_call_label(ctx.emitter, "__rt_strcopy");
    emit_first_char_case_adjust(ctx, "ucfirst", 97, 122, FirstCharAdjust::Uppercase);
    store_if_result(ctx, inst)
}

/// Lowers `lcfirst()` by copying the string and lowercasing the first ASCII byte.
pub(super) fn lower_lcfirst(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "lcfirst")?;
    abi::emit_call_label(ctx.emitter, "__rt_strcopy");
    emit_first_char_case_adjust(ctx, "lcfirst", 65, 90, FirstCharAdjust::Lowercase);
    store_if_result(ctx, inst)
}

/// Lowers `trim()`/`ltrim()`/`rtrim()`/`chop()` for default and explicit masks.
pub(super) fn lower_trim_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    default_runtime_label: &str,
    mask_runtime_label: &str,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 or 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)?;
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, default_runtime_label);
    } else {
        lower_trim_mask_arg(ctx, inst, name)?;
        abi::emit_call_label(ctx.emitter, mask_runtime_label);
    }
    store_if_result(ctx, inst)
}

/// Lowers a two-argument string builtin that directly delegates to a runtime helper.
pub(super) fn lower_binary_string_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_binary_string_args(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `explode(delimiter, string)` into the shared string-array splitter helper.
pub(super) fn lower_explode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let cleanups = plan_split_string_temp_cleanups(ctx, inst)?;
    if !cleanups.is_empty() {
        abi::emit_reserve_temporary_stack(ctx.emitter, cleanups.bytes);
    }
    load_split_pair_args(ctx, inst, "explode", &cleanups)?;
    abi::emit_call_label(ctx.emitter, "__rt_explode");
    emit_split_string_temp_cleanups(ctx, &cleanups);
    store_if_result(ctx, inst)
}

/// Lowers `sscanf(string, format)` into the shared scanner helper.
pub(super) fn lower_sscanf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_str_split(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
    abi::emit_call_label(ctx.emitter, "__rt_str_split");
    store_if_result(ctx, inst)
}

/// Lowers `implode(glue, array)` by selecting the string or integer array helper.
pub(super) fn lower_implode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "implode expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    let runtime_label = implode_runtime_label(ctx, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_implode_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_implode_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `hash(algo, data, binary?)` through the shared runtime digest dispatcher.
pub(super) fn lower_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "hash expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_hash_x86_64(ctx, inst)?,
    }
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    store_if_result(ctx, inst)
}

/// Lowers `hash_hmac(algo, data, key, binary?)` through the shared HMAC runtime dispatcher.
pub(super) fn lower_hash_hmac(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 3 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "hash_hmac expected 3 or 4 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_hmac_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_hash_hmac_x86_64(ctx, inst)?,
    }
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_hmac");
    store_if_result(ctx, inst)
}

/// Lowers `hash_equals(known, user)` through the timing-safe runtime compare helper.
pub(super) fn lower_hash_equals(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_binary_string_args(ctx, inst, "hash_equals")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_equals");
    store_if_result(ctx, inst)
}

/// Lowers `hash_algos()` through the runtime algorithm-list builder.
pub(super) fn lower_hash_algos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::invalid_module(format!(
            "hash_algos expected 0 args, got {}",
            inst.operands.len()
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_algos_list");
    store_if_result(ctx, inst)
}

/// Lowers `hash_init(algo)` and returns a boxed HashContext resource.
pub(super) fn lower_hash_init(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "hash_init", 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "hash_init", string_ptr_reg(ctx), string_len_reg(ctx))?;
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_init");
    store_if_result(ctx, inst)
}

/// Lowers `hash_update(context, data)` through the incremental hash runtime helper.
pub(super) fn lower_hash_update(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "hash_update", 2)?;
    let context = expect_operand(inst, 0)?;
    super::io::load_stream_fd_to_result(ctx, context, "hash_update")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_arg_to_regs(ctx, inst, 1, "hash_update", "x1", "x2")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_arg_to_regs(ctx, inst, 1, "hash_update", "rax", "rdx")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass the hash_update data pointer to the C ABI helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_update");
    store_if_result(ctx, inst)
}

/// Lowers `hash_final(context, binary?)` through the incremental hash finalizer.
pub(super) fn lower_hash_final(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "hash_final expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let context = expect_operand(inst, 0)?;
    super::io::load_stream_fd_to_result(ctx, context, "hash_final")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            materialize_truthy_flag(ctx, inst, 1, "hash_final")?;
            ctx.emitter.instruction("mov x5, x0");                              // pass the raw-output flag to the hash finalizer
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            materialize_truthy_flag(ctx, inst, 1, "hash_final")?;
            ctx.emitter.instruction("mov r10, rax");                            // pass the raw-output flag to the hash finalizer
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_final");
    store_if_result(ctx, inst)
}

/// Lowers `hash_copy(context)` through the incremental hash clone helper.
pub(super) fn lower_hash_copy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "hash_copy", 1)?;
    let context = expect_operand(inst, 0)?;
    super::io::load_stream_fd_to_result(ctx, context, "hash_copy")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the hash context handle to the C ABI helper
    }
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_copy");
    store_if_result(ctx, inst)
}

/// Lowers `crc32(string)` through the shared checksum runtime helper.
pub(super) fn lower_crc32(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "crc32")?;
    abi::emit_call_label(ctx.emitter, "__rt_crc32");
    store_if_result(ctx, inst)
}

/// Lowers `md5(data, binary?)` through the shared crypto-backed runtime helper.
pub(super) fn lower_md5(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_fixed_hash(ctx, inst, "md5", "__rt_md5")
}

/// Lowers `sha1(data, binary?)` through the shared crypto-backed runtime helper.
pub(super) fn lower_sha1(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_fixed_hash(ctx, inst, "sha1", "__rt_sha1")
}

/// Lowers fixed-algorithm hash builtins that share the `__rt_hash` contract.
fn lower_fixed_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 or 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the hash data while materializing the raw-output flag
            materialize_truthy_flag(ctx, inst, 1, name)?;
            ctx.emitter.instruction("mov x5, x0");                              // pass the raw-output flag as the fixed-hash helper's extra argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the hash data into the fixed-hash input registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            materialize_truthy_flag(ctx, inst, 1, name)?;
            ctx.emitter.instruction("mov r10, rax");                            // pass the raw-output flag as the fixed-hash helper's extra argument
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `gzcompress(data, level?)` through inline zlib `compress2` calls.
pub(super) fn lower_gzcompress(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_gzdeflate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_gzinflate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
        Arch::AArch64 => lower_gzinflate_aarch64(ctx, &zero, &zeroed, &fail, &done),
        Arch::X86_64 => lower_gzinflate_x86_64(ctx, &zero, &zeroed, &fail, &done),
    }
    box_string_or_false_result(ctx, "gzinflate");
    store_if_result(ctx, inst)
}

/// Lowers `gzuncompress(data, max_length?)` and boxes zlib failures as PHP false.
pub(super) fn lower_gzuncompress(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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

/// Lowers `long2ip(value)` through the IPv4 formatting runtime helper.
pub(super) fn lower_long2ip(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "long2ip", 1)?;
    let value = expect_operand(inst, 0)?;
    load_as_int(ctx, value, "long2ip")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the IPv4 integer to the formatter helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_long2ip");
    store_if_result(ctx, inst)
}

/// Lowers `ip2long(string)` and boxes invalid-address results as PHP false.
pub(super) fn lower_ip2long(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ip2long")?;
    move_string_result_to_c_abi_pair(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_ip2long");
    box_ip2long_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `inet_ntop()` and `inet_pton()` and boxes invalid-address results as PHP false.
pub(super) fn lower_inet(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_single_string_arg(ctx, inst, name)?;
    move_string_result_to_c_abi_pair(ctx);
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_string_or_false_result(ctx, name);
    store_if_result(ctx, inst)
}

/// Lowers `sprintf(format, values...)` by packing variadic records for `__rt_sprintf`.
pub(super) fn lower_sprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_sprintf_runtime_call(ctx, inst, "sprintf")?;
    store_if_result(ctx, inst)
}

/// Lowers `printf(format, values...)` as `sprintf()` followed by stdout emission.
pub(super) fn lower_printf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_sprintf_runtime_call(ctx, inst, "printf")?;
    emit_printf_write_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `vsprintf(format, values)` through the array-to-sprintf runtime bridge.
pub(super) fn lower_vsprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_vsprintf_runtime_call(ctx, inst, "vsprintf")?;
    store_if_result(ctx, inst)
}

/// Lowers `vprintf(format, values)` as `vsprintf()` followed by stdout emission.
pub(super) fn lower_vprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    emit_vsprintf_runtime_call(ctx, inst, "vprintf")?;
    emit_printf_write_result(ctx);
    store_if_result(ctx, inst)
}

/// Packs sprintf-style operands and calls the shared `__rt_sprintf` formatter.
fn emit_sprintf_runtime_call(
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
pub(super) fn sprintf_spec_cats_for_format(
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
fn parse_sprintf_spec_cats(format: &[u8]) -> Vec<SprintfSpecCat> {
    let mut cats = Vec::new();
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
        while index < format.len()
            && matches!(format[index], b'-' | b'+' | b'0' | b' ' | b'#')
        {
            index += 1;
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
        cats.push(match format[index] {
            b'f' | b'e' | b'g' => SprintfSpecCat::Float,
            b's' => SprintfSpecCat::Str,
            _ => SprintfSpecCat::Int,
        });
        index += 1;
    }
    cats
}

/// Preserves the format string, evaluates the values array, and calls `__rt_vsprintf`.
fn emit_vsprintf_runtime_call(
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

/// Lowers `str_contains()` through `strpos()` and converts found positions to bool.
pub(super) fn lower_str_contains(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_binary_string_args(ctx, inst, "str_contains")?;
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // check whether strpos() found the needle at any non-negative position
            ctx.emitter.instruction("cset x0, ge");                             // normalize the signed strpos() result into a PHP boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // check whether strpos() found the needle at any non-negative position
            ctx.emitter.instruction("setge al");                                // normalize the signed strpos() result into the low boolean byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized boolean byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `strpos()`/`strrpos()` and boxes position-or-false results as Mixed.
pub(super) fn lower_string_position(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_binary_string_args(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_search_result(ctx, name);
    store_if_result(ctx, inst)
}

/// Lowers `substr(string, offset, length?)` with target-local pointer arithmetic.
pub(super) fn lower_substr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    let neg_done = ctx.next_label("substr_neg_done");
    let len_done = ctx.next_label("substr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_aarch64(ctx, inst, &neg_done, &len_done)?,
        Arch::X86_64 => lower_substr_x86_64(ctx, inst, &neg_done, &len_done)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `substr_replace(string, replacement, start, length?)`.
pub(super) fn lower_substr_replace(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 3 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr_replace expected 3 or 4 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_replace_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_substr_replace_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_substr_replace");
    store_if_result(ctx, inst)
}

/// Lowers `str_repeat(string, times)` through the shared runtime helper.
pub(super) fn lower_str_repeat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "str_repeat expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_repeat_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_repeat_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_repeat");
    store_if_result(ctx, inst)
}

/// Lowers `strstr(haystack, needle)` by searching and returning the matching suffix.
pub(super) fn lower_strstr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "strstr expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    let found_label = ctx.next_label("strstr_found");
    let end_label = ctx.next_label("strstr_end");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_strstr_aarch64(ctx, inst, &found_label, &end_label)?,
        Arch::X86_64 => lower_strstr_x86_64(ctx, inst, &found_label, &end_label)?,
    }
    ctx.emitter.label(&end_label);
    store_if_result(ctx, inst)
}

/// Lowers `str_replace()`/`str_ireplace()` with three string operands.
pub(super) fn lower_string_replace(
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
pub(super) fn lower_wordwrap(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
    abi::emit_call_label(ctx.emitter, "__rt_wordwrap");
    store_if_result(ctx, inst)
}

/// Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper.
pub(super) fn lower_str_pad(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
    abi::emit_call_label(ctx.emitter, "__rt_str_pad");
    store_if_result(ctx, inst)
}

/// Lowers `ord()` by returning the first byte of a string or zero for empty input.
pub(super) fn lower_ord(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_chr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_number_format(
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

/// Describes how the first-byte ASCII case helper mutates matched characters.
enum FirstCharAdjust {
    Uppercase,
    Lowercase,
}

/// Returns the target register holding string-result pointers.
fn string_ptr_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rax",
    }
}

/// Returns the target register holding string-result lengths.
fn string_len_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x2",
        Arch::X86_64 => "rdx",
    }
}

/// Loads the sole argument for a string-transform builtin into string result registers.
fn load_single_string_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)
}

/// Loads the first gzip/zlib string operand into the target string-result registers.
fn load_single_gz_string_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)
}

/// Materializes the optional zlib compression level for AArch64 gzip builtins.
fn materialize_gz_level_aarch64(
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
fn materialize_gz_level_x86_64(
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
fn lower_gzcompress_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
fn lower_gzcompress_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    materialize_gz_level_x86_64(ctx, inst, "gzcompress level")?;
    ctx.emitter.instruction("sub rsp, 64");                                     // reserve scratch storage for zlib compress2 state
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                    // save the compression level
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                    // save the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                   // save the source length
    ctx.emitter.instruction("mov rdi, rdx");                                    // pass the source length to compressBound
    ctx.emitter.instruction("call compressBound");                              // compute the worst-case compressed byte length
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");                   // seed destLen with the output capacity
    ctx.emitter.instruction("call __rt_heap_alloc");                            // allocate the compressed-data buffer
    ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the x86_64 string heap kind word
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
fn lower_gzdeflate_aarch64(
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
fn lower_gzdeflate_x86_64(
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
    ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the x86_64 string heap kind word
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

/// Emits AArch64 `gzinflate()` inline raw-inflate calls.
fn lower_gzinflate_aarch64(
    ctx: &mut FunctionContext<'_>,
    zero: &str,
    zeroed: &str,
    fail: &str,
    done: &str,
) {
    ctx.emitter.instruction("sub sp, sp, #160");                                // reserve z_stream storage plus scratch slots
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

    ctx.emitter.instruction("mov x9, #0");                                      // initialize the z_stream clear index
    ctx.emitter.label(zero);
    ctx.emitter.instruction("cmp x9, #112");                                    // check whether every z_stream byte is cleared
    ctx.emitter.instruction(&format!("b.ge {}", zeroed));                       // continue after the z_stream has been zeroed
    ctx.emitter.instruction("strb wzr, [sp, x9]");                              // clear one z_stream byte
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance the z_stream clear index
    ctx.emitter.instruction(&format!("b {}", zero));                            // keep clearing the z_stream
    ctx.emitter.label(zeroed);

    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer
    ctx.emitter.instruction("mov x1, #-15");                                    // request raw inflate with negative window bits
    abi::emit_symbol_address(ctx.emitter, "x2", "_zlib_version");
    ctx.emitter.instruction("mov x3, #112");                                    // pass sizeof(z_stream)
    ctx.emitter.bl_c("inflateInit2_");                                          // initialize the raw-inflate zlib stream
    ctx.emitter.instruction("ldr x9, [sp, #112]");                              // reload the source pointer
    ctx.emitter.instruction("str x9, [sp, #0]");                                // set z_stream.next_in
    ctx.emitter.instruction("ldr x9, [sp, #120]");                              // reload the source length
    ctx.emitter.instruction("str w9, [sp, #8]");                                // set z_stream.avail_in
    ctx.emitter.instruction("ldr x9, [sp, #128]");                              // reload the destination pointer
    ctx.emitter.instruction("str x9, [sp, #24]");                               // set z_stream.next_out
    ctx.emitter.instruction("ldr x9, [sp, #144]");                              // reload the destination capacity
    ctx.emitter.instruction("str w9, [sp, #32]");                               // set z_stream.avail_out
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer to inflate
    ctx.emitter.instruction("mov x1, #4");                                      // request a final inflate pass
    ctx.emitter.bl_c("inflate");                                                // decompress the full input
    ctx.emitter.instruction("str x0, [sp, #136]");                              // save the inflate status code
    ctx.emitter.instruction("ldr x2, [sp, #40]");                               // read z_stream.total_out
    ctx.emitter.instruction("str x2, [sp, #152]");                              // save the inflated length across inflateEnd
    ctx.emitter.instruction("mov x0, sp");                                      // pass the z_stream pointer to inflateEnd
    ctx.emitter.bl_c("inflateEnd");                                             // release zlib's internal inflate state
    ctx.emitter.instruction("ldr x9, [sp, #136]");                              // reload the inflate status code
    ctx.emitter.instruction("cmp x9, #1");                                      // check for Z_STREAM_END success
    ctx.emitter.instruction(&format!("b.ne {}", fail));                         // return false for zlib inflate failures
    ctx.emitter.instruction("ldr x1, [sp, #128]");                              // return the decompressed string pointer
    ctx.emitter.instruction("ldr x2, [sp, #152]");                              // return the decompressed string length
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the failure sentinel after success
    ctx.emitter.label(fail);
    ctx.emitter.instruction("mov x1, #0");                                      // use a null pointer as the failure sentinel
    ctx.emitter.instruction("mov x2, #0");                                      // clear the failure string length
    ctx.emitter.label(done);
    ctx.emitter.instruction("add sp, sp, #160");                                // release z_stream scratch storage
}

/// Emits x86_64 `gzinflate()` inline raw-inflate calls.
fn lower_gzinflate_x86_64(
    ctx: &mut FunctionContext<'_>,
    zero: &str,
    zeroed: &str,
    fail: &str,
    done: &str,
) {
    let sized = format!("{}_sized", zero);
    ctx.emitter.instruction("sub rsp, 160");                                    // reserve z_stream storage plus scratch slots
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
    ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the x86_64 string heap kind word
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
    ctx.emitter.instruction("mov esi, -15");                                    // request raw inflate with negative window bits
    abi::emit_symbol_address(ctx.emitter, "rdx", "_zlib_version");
    ctx.emitter.instruction("mov ecx, 112");                                    // pass sizeof(z_stream)
    ctx.emitter.instruction("call inflateInit2_");                              // initialize the raw-inflate zlib stream
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 112]");                   // reload the source pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");                     // set z_stream.next_in
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                   // reload the source length
    ctx.emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                    // set z_stream.avail_in
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                   // reload the destination pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], r9");                    // set z_stream.next_out
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 144]");                   // reload the destination capacity
    ctx.emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                   // set z_stream.avail_out
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer to inflate
    ctx.emitter.instruction("mov esi, 4");                                      // request a final inflate pass
    ctx.emitter.instruction("call inflate");                                    // decompress the full input
    ctx.emitter.instruction("mov QWORD PTR [rsp + 136], rax");                  // save the inflate status code
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // read z_stream.total_out
    ctx.emitter.instruction("mov QWORD PTR [rsp + 152], rax");                  // save the inflated length across inflateEnd
    ctx.emitter.instruction("mov rdi, rsp");                                    // pass the z_stream pointer to inflateEnd
    ctx.emitter.instruction("call inflateEnd");                                 // release zlib's internal inflate state
    ctx.emitter.instruction("cmp QWORD PTR [rsp + 136], 1");                    // check for Z_STREAM_END success
    ctx.emitter.instruction(&format!("jne {}", fail));                          // return false for zlib inflate failures
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                  // return the decompressed string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 152]");                  // return the decompressed string length
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the failure sentinel after success
    ctx.emitter.label(fail);
    ctx.emitter.instruction("xor eax, eax");                                    // use a null pointer as the failure sentinel
    ctx.emitter.instruction("xor edx, edx");                                    // clear the failure string length
    ctx.emitter.label(done);
    ctx.emitter.instruction("add rsp, 160");                                    // release z_stream scratch storage
}

/// Emits AArch64 `gzuncompress()` inline zlib-wrapped inflate calls.
fn lower_gzuncompress_aarch64(ctx: &mut FunctionContext<'_>, ok: &str, after: &str) {
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
fn lower_gzuncompress_x86_64(ctx: &mut FunctionContext<'_>, ok: &str, after: &str) {
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
    ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the x86_64 string heap kind word
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

/// Preserves the trim source string while loading the explicit character mask.
fn lower_trim_mask_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // preserve the source string pointer while loading the trim mask
            ctx.emitter.instruction("str x2, [sp, #-16]!");                     // preserve the source string length while loading the trim mask
            load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the trim-mask pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the trim-mask length as the secondary string argument
            ctx.emitter.instruction("ldr x2, [sp], #16");                       // restore the source string length after loading the mask
            ctx.emitter.instruction("ldr x1, [sp], #16");                       // restore the source string pointer after loading the mask
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the trim-mask pointer as the secondary string argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the trim-mask length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    Ok(())
}

/// Materializes two string operands into the runtime helper's target ABI registers.
fn load_binary_string_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the first string pointer and length while loading the second
            load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the second string pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the second string length as the secondary string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the first string pointer and length into primary argument registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the second string length as the fourth SysV string argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the second string pointer as the third SysV string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    Ok(())
}

/// Returns a string operand after validating the EIR builtin call shape.
fn expect_string_operand(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<ValueId> {
    let value = expect_operand(inst, index)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::Str {
        return Ok(value);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} arg {} for PHP type {:?}",
        name,
        index + 1,
        ty
    )))
}

/// Emits the AArch64 inline substring pointer/length calculation.
fn lower_substr_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_aarch64(ctx, inst)?;
    if inst.operands.len() >= 3 {
        let length = expect_operand(inst, 2)?;
        load_as_int(ctx, length, "substr length")?;
        ctx.emitter.instruction("mov x3, x0");                                  // move the explicit substring length into the clamp register
    } else {
        ctx.emitter.instruction("mov x3, #-1");                                 // use -1 as the sentinel for an omitted substring length
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the substring offset after optional length materialization
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string pointer and length
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the requested offset is negative
    ctx.emitter.instruction(&format!("b.ge {}", neg_done));                     // skip tail-relative offset adjustment for non-negative offsets
    ctx.emitter.instruction("add x0, x2, x0");                                  // convert the negative offset into a tail-relative byte index
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the tail-relative offset still points before the string
    ctx.emitter.instruction("csel x0, xzr, x0, lt");                            // clamp underflowing offsets back to the start of the string
    ctx.emitter.label(neg_done);
    ctx.emitter.instruction("cmp x0, x2");                                      // compare the final offset against the full source-string length
    ctx.emitter.instruction("csel x0, x2, x0, gt");                             // clamp offsets past the end to the source-string length
    ctx.emitter.instruction("add x1, x1, x0");                                  // advance the result pointer to the selected substring start
    ctx.emitter.instruction("sub x2, x2, x0");                                  // compute the remaining byte length after the selected offset
    ctx.emitter.instruction("cmn x3, #1");                                      // check whether the optional length argument was omitted
    ctx.emitter.instruction(&format!("b.eq {}", len_done));                     // keep the full remaining tail when no explicit length was provided
    ctx.emitter.instruction("cmp x3, #0");                                      // check whether the requested substring length is negative
    ctx.emitter.instruction("csel x3, xzr, x3, lt");                            // clamp negative requested lengths to zero
    ctx.emitter.instruction("cmp x3, x2");                                      // compare requested length against the remaining tail length
    ctx.emitter.instruction("csel x2, x3, x2, lt");                             // shrink the result length when the requested length is shorter
    ctx.emitter.label(len_done);
    Ok(())
}

/// Loads the source string and offset for AArch64 `substr()` lowering.
fn load_substr_string_and_offset_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let offset = expect_operand(inst, 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "substr", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing numeric arguments
    load_as_int(ctx, offset, "substr offset")?;
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // preserve the substring offset while materializing the optional length
    Ok(())
}

/// Emits the x86_64 inline substring pointer/length calculation.
fn lower_substr_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_x86_64(ctx, inst)?;
    if inst.operands.len() >= 3 {
        let length = expect_operand(inst, 2)?;
        load_as_int(ctx, length, "substr length")?;
        ctx.emitter.instruction("mov rcx, rax");                                // move the explicit substring length into the clamp register
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rcx", -1);
    }
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the requested offset is negative
    ctx.emitter.instruction(&format!("jge {}", neg_done));                      // skip tail-relative offset adjustment for non-negative offsets
    ctx.emitter.instruction("add rax, rsi");                                    // convert the negative offset into a tail-relative byte index
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the tail-relative offset still points before the string
    ctx.emitter.instruction("mov r8, 0");                                       // materialize zero for offset and length clamping
    ctx.emitter.instruction("cmovl rax, r8");                                   // clamp underflowing offsets back to the start of the string
    ctx.emitter.label(neg_done);
    ctx.emitter.instruction("cmp rax, rsi");                                    // compare the final offset against the full source-string length
    ctx.emitter.instruction("cmovg rax, rsi");                                  // clamp offsets past the end to the source-string length
    ctx.emitter.instruction("add rdi, rax");                                    // advance the result pointer to the selected substring start
    ctx.emitter.instruction("sub rsi, rax");                                    // compute the remaining byte length after the selected offset
    ctx.emitter.instruction("cmp rcx, -1");                                     // check whether the optional length argument was omitted
    ctx.emitter.instruction(&format!("je {}", len_done));                       // keep the full remaining tail when no explicit length was provided
    ctx.emitter.instruction("cmp rcx, 0");                                      // check whether the requested substring length is negative
    ctx.emitter.instruction("mov r8, 0");                                       // materialize zero for negative length clamping
    ctx.emitter.instruction("cmovl rcx, r8");                                   // clamp negative requested lengths to zero
    ctx.emitter.instruction("cmp rcx, rsi");                                    // compare requested length against the remaining tail length
    ctx.emitter.instruction("cmovl rsi, rcx");                                  // shrink the result length when the requested length is shorter
    ctx.emitter.label(len_done);
    ctx.emitter.instruction("mov rax, rdi");                                    // return the selected substring pointer in the string result register
    ctx.emitter.instruction("mov rdx, rsi");                                    // return the selected substring length in the string result register
    Ok(())
}

/// Loads the source string and offset for x86_64 `substr()` lowering.
fn load_substr_string_and_offset_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let offset = expect_operand(inst, 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "substr", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, offset, "substr offset")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    Ok(())
}

/// Materializes AArch64 `str_repeat()` runtime arguments.
fn lower_str_repeat_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_repeat")?;
    let times = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the repeat count
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov x3, x0");                                      // pass the repeat count as the third string-helper argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string into runtime argument registers
    Ok(())
}

/// Materializes x86_64 `str_repeat()` runtime arguments.
fn lower_str_repeat_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_repeat")?;
    let times = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the repeat count as the extra x86_64 runtime argument
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Emits AArch64 `strstr()` search and suffix reconstruction.
fn lower_strstr_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    found_label: &str,
    end_label: &str,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "strstr", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack while materializing the needle string
    load_string_arg_to_regs(ctx, inst, 1, "strstr", "x1", "x2")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the needle pointer as the secondary string argument
    ctx.emitter.instruction("mov x4, x2");                                      // pass the needle length as the secondary string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack into primary string argument registers
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack while strpos() returns the match offset
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack for suffix reconstruction
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether strpos() returned a valid match offset
    ctx.emitter.instruction(&format!("b.ge {}", found_label));                  // build the matching suffix when the needle was found
    ctx.emitter.instruction("mov x1, #0");                                      // return a null pointer for the empty not-found string
    ctx.emitter.instruction("mov x2, #0");                                      // return zero length for the empty not-found string
    ctx.emitter.instruction(&format!("b {}", end_label));                       // skip suffix pointer adjustment for a miss
    ctx.emitter.label(found_label);
    ctx.emitter.instruction("add x1, x1, x0");                                  // advance the haystack pointer to the matching suffix
    ctx.emitter.instruction("sub x2, x2, x0");                                  // shrink the haystack length to the matching suffix length
    Ok(())
}

/// Emits x86_64 `strstr()` search and suffix reconstruction.
fn lower_strstr_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    found_label: &str,
    end_label: &str,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "strstr", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 1, "strstr", "rax", "rdx")?;
    ctx.emitter.instruction("mov r8, rax");                                     // preserve the needle pointer while restoring the haystack
    ctx.emitter.instruction("mov r9, rdx");                                     // preserve the needle length while restoring the haystack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the haystack pointer as the first SysV string argument
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the haystack length as the second SysV string argument
    ctx.emitter.instruction("mov rdx, r8");                                     // pass the needle pointer as the third SysV string argument
    ctx.emitter.instruction("mov rcx, r9");                                     // pass the needle length as the fourth SysV string argument
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    ctx.emitter.instruction("mov r8, rax");                                     // preserve the signed match offset while restoring the haystack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("cmp r8, 0");                                       // check whether strpos() returned a valid match offset
    ctx.emitter.instruction(&format!("jge {}", found_label));                   // build the matching suffix when the needle was found
    ctx.emitter.instruction("xor eax, eax");                                    // return a null pointer for the empty not-found string
    ctx.emitter.instruction("xor edx, edx");                                    // return zero length for the empty not-found string
    ctx.emitter.instruction(&format!("jmp {}", end_label));                     // skip suffix pointer adjustment for a miss
    ctx.emitter.label(found_label);
    ctx.emitter.instruction("add rax, r8");                                     // advance the haystack pointer to the matching suffix
    ctx.emitter.instruction("sub rdx, r8");                                     // shrink the haystack length to the matching suffix length
    Ok(())
}

/// Materializes AArch64 `hash()` runtime arguments.
fn lower_hash_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "hash", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the algorithm string while materializing the data string
    load_string_arg_to_regs(ctx, inst, 1, "hash", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the data string while materializing the binary flag
    materialize_truthy_flag(ctx, inst, 2, "hash")?;
    ctx.emitter.instruction("mov x5, x0");                                      // pass the raw-output flag as the fifth hash argument
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the data string into secondary hash argument registers
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the algorithm string into primary hash argument registers
    Ok(())
}

/// Materializes x86_64 `hash()` runtime arguments.
fn lower_hash_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "hash", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 1, "hash", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    materialize_truthy_flag(ctx, inst, 2, "hash")?;
    ctx.emitter.instruction("mov r10, rax");                                    // pass the raw-output flag as the hash helper's extra argument
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes AArch64 `hash_hmac()` runtime arguments.
fn lower_hash_hmac_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "hash_hmac", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the algorithm string while materializing HMAC data
    load_string_arg_to_regs(ctx, inst, 1, "hash_hmac", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the HMAC data string while materializing the key
    load_string_arg_to_regs(ctx, inst, 2, "hash_hmac", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the HMAC key string while materializing the binary flag
    materialize_truthy_flag(ctx, inst, 3, "hash_hmac")?;
    ctx.emitter.instruction("mov x7, x0");                                      // pass the raw-output flag to the HMAC helper
    ctx.emitter.instruction("ldp x5, x6, [sp], #16");                           // restore the HMAC key string into key argument registers
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the HMAC data string into data argument registers
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the algorithm string into algorithm argument registers
    Ok(())
}

/// Materializes x86_64 `hash_hmac()` runtime arguments.
fn lower_hash_hmac_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "hash_hmac", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 1, "hash_hmac", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 2, "hash_hmac", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    materialize_truthy_flag(ctx, inst, 3, "hash_hmac")?;
    ctx.emitter.instruction("mov rcx, rax");                                    // pass the raw-output flag to the HMAC helper
    abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes delimiter/payload string pairs for split-style array helpers.
fn load_split_pair_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    cleanups: &SplitStringTempCleanups,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_split_pair_args_aarch64(ctx, inst, name, cleanups),
        Arch::X86_64 => load_split_pair_args_x86_64(ctx, inst, name, cleanups),
    }
}

/// Materializes AArch64 delimiter and subject strings for `explode()`.
fn load_split_pair_args_aarch64(
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
fn load_split_pair_args_x86_64(
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
fn plan_split_string_temp_cleanups(
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
fn value_string_coercion_needs_temp_cleanup(
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
fn save_split_string_temp(
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
fn emit_split_string_temp_cleanups(
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

/// Materializes a builtin argument as a PHP string in caller-selected registers.
pub(super) fn load_string_arg_to_regs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let value = expect_operand(inst, index)?;
    load_value_as_string_to_regs(ctx, value, name, ptr_reg, len_reg)
}

/// Materializes an arbitrary EIR value as a PHP string in caller-selected registers.
pub(super) fn load_value_as_string_to_regs(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let raw_ty = ctx.value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
        move_string_result_to_regs(ctx, ptr_reg, len_reg);
        return Ok(());
    }
    let ty = raw_ty.codegen_repr();
    match ty {
        PhpType::Str => ctx.load_string_value_to_regs(value, ptr_reg, len_reg),
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            emit_loaded_bool_string_result(ctx)?;
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            emit_empty_string_result(ctx);
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            emit_loaded_tagged_scalar_string_result(ctx)?;
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_borrowed_string_to_regs(ctx, value)?;
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} string coercion for PHP type {:?}",
            name, other
        ))),
    }
}

/// Materializes a `Mixed`/union value as a borrowed PHP string for builtin arguments.
///
/// String payloads are borrowed directly from the boxed cell instead of being
/// persisted. Scalar payloads stringify into concat scratch storage, which is
/// reset by the usual request/function concat-base cleanup.
fn emit_mixed_borrowed_string_to_regs(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_mixed_borrowed_string_aarch64(ctx, value),
        Arch::X86_64 => emit_mixed_borrowed_string_x86_64(ctx, value),
    }
}

/// Emits AArch64 borrowed string coercion for a boxed `Mixed` value.
fn emit_mixed_borrowed_string_aarch64(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let from_int = ctx.next_label("mixed_arg_string_from_int");
    let from_string = ctx.next_label("mixed_arg_string_from_string");
    let from_float = ctx.next_label("mixed_arg_string_from_float");
    let from_bool = ctx.next_label("mixed_arg_string_from_bool");
    let false_bool = ctx.next_label("mixed_arg_string_false_bool");
    let done = ctx.next_label("mixed_arg_string_done");
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the boxed argument is an integer payload
    ctx.emitter.instruction(&format!("b.eq {}", from_int));                     // stringify integer payloads through the concat-backed itoa helper
    ctx.emitter.instruction("cmp x0, #1");                                      // check whether the boxed argument already holds a string payload
    ctx.emitter.instruction(&format!("b.eq {}", from_string));                  // borrow string payloads directly from the boxed cell
    ctx.emitter.instruction("cmp x0, #2");                                      // check whether the boxed argument is a float payload
    ctx.emitter.instruction(&format!("b.eq {}", from_float));                   // stringify float payloads through the concat-backed ftoa helper
    ctx.emitter.instruction("cmp x0, #3");                                      // check whether the boxed argument is a boolean payload
    ctx.emitter.instruction(&format!("b.eq {}", from_bool));                    // stringify boolean payloads using PHP scalar rules
    ctx.emitter.instruction("mov x1, xzr");                                     // use an empty string pointer for null or unsupported boxed payloads
    ctx.emitter.instruction("mov x2, xzr");                                     // use zero length for null or unsupported boxed payloads
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the normalized empty string

    ctx.emitter.label(&from_int);
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed integer payload to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the concat-backed integer string

    ctx.emitter.label(&from_string);
    ctx.emitter.instruction(&format!("b {}", done));                            // x1/x2 already hold the borrowed string payload

    ctx.emitter.label(&from_float);
    ctx.emitter.instruction("fmov d0, x1");                                     // move unboxed float bits into the FP argument register
    abi::emit_call_label(ctx.emitter, "__rt_ftoa");
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the concat-backed float string

    ctx.emitter.label(&from_bool);
    ctx.emitter.instruction(&format!("cbz x1, {}", false_bool));                // false stringifies to an empty string
    ctx.emitter.instruction("mov x0, x1");                                      // pass true as integer 1 to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the concat-backed true string

    ctx.emitter.label(&false_bool);
    ctx.emitter.instruction("mov x1, xzr");                                     // false uses an empty string pointer
    ctx.emitter.instruction("mov x2, xzr");                                     // false uses zero string length

    ctx.emitter.label(&done);
    Ok(())
}

/// Emits x86_64 borrowed string coercion for a boxed `Mixed` value.
fn emit_mixed_borrowed_string_x86_64(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let from_int = ctx.next_label("mixed_arg_string_from_int");
    let from_string = ctx.next_label("mixed_arg_string_from_string");
    let from_float = ctx.next_label("mixed_arg_string_from_float");
    let from_bool = ctx.next_label("mixed_arg_string_from_bool");
    let false_bool = ctx.next_label("mixed_arg_string_false_bool");
    let done = ctx.next_label("mixed_arg_string_done");
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the boxed argument is an integer payload
    ctx.emitter.instruction(&format!("je {}", from_int));                       // stringify integer payloads through the concat-backed itoa helper
    ctx.emitter.instruction("cmp rax, 1");                                      // check whether the boxed argument already holds a string payload
    ctx.emitter.instruction(&format!("je {}", from_string));                    // borrow string payloads directly from the boxed cell
    ctx.emitter.instruction("cmp rax, 2");                                      // check whether the boxed argument is a float payload
    ctx.emitter.instruction(&format!("je {}", from_float));                     // stringify float payloads through the concat-backed ftoa helper
    ctx.emitter.instruction("cmp rax, 3");                                      // check whether the boxed argument is a boolean payload
    ctx.emitter.instruction(&format!("je {}", from_bool));                      // stringify boolean payloads using PHP scalar rules
    ctx.emitter.instruction("xor eax, eax");                                    // use an empty string pointer for null or unsupported boxed payloads
    ctx.emitter.instruction("xor edx, edx");                                    // use zero length for null or unsupported boxed payloads
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the normalized empty string

    ctx.emitter.label(&from_int);
    ctx.emitter.instruction("mov rax, rdi");                                    // pass the unboxed integer payload to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the concat-backed integer string

    ctx.emitter.label(&from_string);
    ctx.emitter.instruction("mov rax, rdi");                                    // return the borrowed string pointer from the boxed payload
    ctx.emitter.instruction(&format!("jmp {}", done));                          // rdx already holds the borrowed string length

    ctx.emitter.label(&from_float);
    ctx.emitter.instruction("movq xmm0, rdi");                                  // move unboxed float bits into the FP argument register
    abi::emit_call_label(ctx.emitter, "__rt_ftoa");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the concat-backed float string

    ctx.emitter.label(&from_bool);
    ctx.emitter.instruction("test rdi, rdi");                                   // false stringifies to an empty string
    ctx.emitter.instruction(&format!("je {}", false_bool));                     // branch to the empty-string result for false
    ctx.emitter.instruction("mov rax, rdi");                                    // pass true as integer 1 to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the concat-backed true string

    ctx.emitter.label(&false_bool);
    ctx.emitter.instruction("xor eax, eax");                                    // false uses an empty string pointer
    ctx.emitter.instruction("xor edx, edx");                                    // false uses zero string length

    ctx.emitter.label(&done);
    Ok(())
}

/// Converts the loaded boolean result to PHP string ABI registers.
fn emit_loaded_bool_string_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let false_label = ctx.next_label("bool_arg_to_str_false");
    let done_label = ctx.next_label("bool_arg_to_str_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after true conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the boolean payload is false
            ctx.emitter.instruction(&format!("je {}", false_label));            // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after true conversion
        }
    }
    ctx.emitter.label(&false_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Converts the loaded tagged scalar result to PHP string ABI registers.
fn emit_loaded_tagged_scalar_string_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let null_label = ctx.next_label("tagged_arg_to_str_null");
    let done_label = ctx.next_label("tagged_arg_to_str_done");
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_label);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes a valid empty PHP string in the target ABI string-result registers.
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let (label, _) = ctx.data.add_string(b"");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Moves the target ABI string result pair into caller-selected registers when needed.
fn move_string_result_to_regs(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    let (result_ptr_reg, result_len_reg) = abi::string_result_regs(ctx.emitter);
    if ptr_reg != result_ptr_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", ptr_reg, result_ptr_reg)); // move the cast string pointer into the requested argument register
    }
    if len_reg != result_len_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", len_reg, result_len_reg)); // move the cast string length into the requested argument register
    }
}

/// Materializes an optional PHP truthiness flag into the integer result register.
pub(super) fn materialize_truthy_flag(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<()> {
    if inst.operands.len() <= index {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return Ok(());
    }
    let value = expect_operand(inst, index)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return Ok(());
    }
    match raw_ty.codegen_repr() {
        PhpType::Bool | PhpType::Int | PhpType::Pointer(_) => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} truthiness flag for PHP type {:?}",
                name,
                other
            )))
        }
    }
    Ok(())
}

/// Moves the standard string result pair into the C-style pointer/length argument pair.
fn move_string_result_to_c_abi_pair(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the string pointer as the first C ABI argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the string length as the second C ABI argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the string pointer as the first SysV argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the string length as the second SysV argument
        }
    }
}

/// Boxes an `ip2long()` integer result or invalid-address sentinel into Mixed form.
fn box_ip2long_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("ip2long_false");
    let done_label = ctx.next_label("ip2long_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether ip2long() returned the invalid-address sentinel
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false for invalid addresses
            ctx.emitter.instruction("mov x1, x0");                              // pass the parsed IPv4 integer as the Mixed payload
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after a valid parse
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // false payload = 0 for invalid addresses
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether ip2long() returned the invalid-address sentinel
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false for invalid addresses
            ctx.emitter.instruction("mov rdi, rax");                            // pass the parsed IPv4 integer as the Mixed payload
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after a valid parse
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // false payload = 0 for invalid addresses
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a string result or null-pointer failure sentinel into Mixed form.
fn box_string_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // a null string pointer means the conversion failed
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after a valid string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // false payload = 0 for failed conversion
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a null string pointer means the conversion failed
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false for failed conversions
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after a valid string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // false payload = 0 for failed conversion
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Materializes primary input and pattern strings for scanner-style helpers.
fn load_input_and_pattern_args(
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
fn load_input_and_pattern_args_aarch64(
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
fn load_input_and_pattern_args_x86_64(
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
fn lower_str_split_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_split")?;
    ctx.load_string_value_to_regs(source, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the chunk length
    materialize_str_split_length_aarch64(ctx, inst)?;
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string before calling the splitter helper
    Ok(())
}

/// Materializes x86_64 source string and optional chunk length for `str_split()`.
fn lower_str_split_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_split")?;
    ctx.load_string_value_to_regs(source, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    materialize_str_split_length_x86_64(ctx, inst)?;
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the AArch64 optional `str_split()` chunk length.
fn materialize_str_split_length_aarch64(
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
fn materialize_str_split_length_x86_64(
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
fn implode_runtime_label(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<&'static str> {
    let array = expect_operand(inst, 1)?;
    match ctx.value_php_type(array)? {
        PhpType::Array(elem_ty) => match elem_ty.codegen_repr() {
            PhpType::Int | PhpType::Bool => Ok("__rt_implode_int"),
            PhpType::Str | PhpType::Mixed | PhpType::Never => Ok("__rt_implode"),
            other => Err(CodegenIrError::unsupported(format!(
                "implode array element PHP type {:?}",
                other
            ))),
        },
        PhpType::Mixed | PhpType::Union(_) => Ok("__rt_implode"),
        other => Err(CodegenIrError::unsupported(format!(
            "implode array PHP type {:?}",
            other
        ))),
    }
}

/// Materializes AArch64 glue and array arguments for `implode()`.
fn lower_implode_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let glue = expect_string_operand(ctx, inst, 0, "implode")?;
    let array = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(glue, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the glue string while materializing the array argument
    load_implode_array_aarch64(ctx, array)?;
    ctx.emitter.instruction("mov x3, x0");                                      // pass the indexed array pointer as the third implode argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the glue string into primary implode argument registers
    Ok(())
}

/// Materializes x86_64 glue and array arguments for `implode()`.
fn lower_implode_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let glue = expect_string_operand(ctx, inst, 0, "implode")?;
    let array = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(glue, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_implode_array_x86_64(ctx, array)?;
    ctx.emitter.instruction("mov rdx, rax");                                    // pass the indexed array pointer as the third implode argument
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    Ok(())
}

/// Loads the raw indexed-array payload consumed by `implode()` on AArch64.
fn load_implode_array_aarch64(
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
        _ => {
            ctx.load_value_to_reg(array, "x0")?;
            Ok(())
        }
    }
}

/// Loads the raw indexed-array payload consumed by `implode()` on x86_64.
fn load_implode_array_x86_64(
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
        _ => {
            ctx.load_value_to_reg(array, "rax")?;
            Ok(())
        }
    }
}

/// Materializes AArch64 `substr_replace()` runtime arguments.
fn lower_substr_replace_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "substr_replace")?;
    let replacement = expect_string_operand(ctx, inst, 1, "substr_replace")?;
    let start = expect_operand(inst, 2)?;
    ctx.load_string_value_to_regs(subject, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject string while materializing replacement and slice bounds
    ctx.load_string_value_to_regs(replacement, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the replacement string while materializing slice bounds
    load_as_int(ctx, start, "substr_replace start")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_substr_replace_length_aarch64(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore replacement into the secondary runtime string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore subject into the primary runtime string argument
    Ok(())
}

/// Materializes x86_64 `substr_replace()` runtime arguments.
fn lower_substr_replace_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "substr_replace")?;
    let replacement = expect_string_operand(ctx, inst, 1, "substr_replace")?;
    let start = expect_operand(inst, 2)?;
    ctx.load_string_value_to_regs(subject, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.load_string_value_to_regs(replacement, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, start, "substr_replace start")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_substr_replace_length_x86_64(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the AArch64 optional `substr_replace()` length argument.
fn materialize_substr_replace_length_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let length = expect_operand(inst, 3)?;
        load_as_int(ctx, length, "substr_replace length")?;
        ctx.emitter.instruction("mov x7, x0");                                  // pass the explicit replacement length to the runtime helper
    } else {
        ctx.emitter.instruction("mov x7, #-1");                                 // use -1 sentinel so replacement runs through the subject end
    }
    Ok(())
}

/// Materializes the x86_64 optional `substr_replace()` length argument.
fn materialize_substr_replace_length_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let length = expect_operand(inst, 3)?;
        load_as_int(ctx, length, "substr_replace length")?;
        ctx.emitter.instruction("mov r8, rax");                                 // pass the explicit replacement length to the runtime helper
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "r8", -1);
    }
    Ok(())
}

/// Materializes AArch64 `str_replace`-family runtime arguments.
fn lower_string_replace_aarch64(
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
fn lower_string_replace_x86_64(
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
fn lower_str_pad_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
fn materialize_str_pad_pad_string_aarch64(
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
fn materialize_str_pad_type_aarch64(
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
fn lower_str_pad_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
fn materialize_str_pad_pad_string_x86_64(
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
fn materialize_str_pad_type_x86_64(
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
fn lower_wordwrap_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
fn materialize_wordwrap_width_aarch64(
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
fn materialize_wordwrap_break_aarch64(
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
fn lower_wordwrap_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
fn materialize_wordwrap_width_x86_64(
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
fn materialize_wordwrap_break_x86_64(
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

/// Packs one printf-family variadic operand into the runtime's 16-byte tagged record.
pub(super) fn pack_sprintf_like_arg(
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
fn pack_static_sprintf_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &ty, owner),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &ty, owner),
    }
}

/// Loads an operand as the integer payload consumed by integer printf specifiers.
fn load_sprintf_arg_as_int(
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
fn load_sprintf_arg_as_float(
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
fn load_sprintf_arg_as_string(
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
fn pack_sprintf_int_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &PhpType::Int, "sprintf"),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &PhpType::Int, "sprintf"),
    }
}

/// Packs the loaded floating result as a printf-family record.
fn pack_sprintf_float_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &PhpType::Float, "sprintf"),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &PhpType::Float, "sprintf"),
    }
}

/// Packs the loaded string result as a printf-family record.
fn pack_sprintf_string_arg(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => pack_sprintf_arg_aarch64(ctx, &PhpType::Str, "sprintf"),
        Arch::X86_64 => pack_sprintf_arg_x86_64(ctx, &PhpType::Str, "sprintf"),
    }
}

/// Packs one AArch64 sprintf operand from result registers into `[value, tag]`.
fn pack_sprintf_arg_aarch64(
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
        _other => {
            ctx.emitter.instruction("str xzr, [sp, #-16]!");                    // push a zero payload for an unsupported sprintf operand
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // tag the unsupported sprintf operand as integer zero
        }
    }
    let _ = owner;
    Ok(())
}

/// Packs one x86_64 sprintf operand from result registers into `[value, tag]`.
fn pack_sprintf_arg_x86_64(
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
fn emit_printf_write_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #1");                              // pass stdout as the destination file descriptor
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, x2");                              // return the formatted byte count as printf()'s integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, rdx");                             // preserve the formatted byte count across syscall-clobbered registers
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer as the write buffer
            ctx.emitter.instruction("mov rdx, r8");                             // pass the formatted string length as the write byte count
            ctx.emitter.instruction("mov edi, 1");                              // pass stdout as the destination file descriptor
            ctx.emitter.instruction("mov eax, 1");                              // select Linux x86_64 syscall 1 for write
            ctx.emitter.instruction("syscall");                                 // write the formatted printf() result to stdout
            ctx.emitter.instruction("mov rax, r8");                             // return the formatted byte count as printf()'s integer result
        }
    }
}

/// Boxes a raw string-search position result into the Mixed pointer representation.
fn box_search_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let found_label = ctx.next_label(&format!("{}_found", label_prefix));
    let end_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a valid non-negative match offset from the not-found sentinel
            ctx.emitter.instruction(&format!("b.ge {}", found_label));          // box a found offset as an integer result
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip integer boxing after producing the false result
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x1, x0");                              // move the found offset into the mixed helper payload register
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for integer mixed boxes
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a valid non-negative match offset from the not-found sentinel
            ctx.emitter.instruction(&format!("jge {}", found_label));           // box a found offset as an integer result
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip integer boxing after producing the false result
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rdi, rax");                            // move the found offset into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for integer mixed boxes
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
    }
}

/// Boxes the raw `grapheme_strrev()` runtime result as PHP `string|false`.
fn box_grapheme_strrev_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("grapheme_strrev_false");
    let done_label = ctx.next_label("grapheme_strrev_done");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box false when grapheme scanning reports a null string pointer
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after a successful grapheme reversal
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // false payload = 0 for grapheme_strrev() failure
            ctx.emitter.instruction("mov x2, #0");                              // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = bool false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test the returned string pointer for the failure sentinel
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when grapheme scanning reports a null string pointer
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after a successful grapheme reversal
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // false payload = 0 for grapheme_strrev() failure
            ctx.emitter.instruction("xor esi, esi");                            // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = bool false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Emits target-aware first-byte ASCII case adjustment for `ucfirst()` and `lcfirst()`.
fn emit_first_char_case_adjust(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
    lower_bound: u8,
    upper_bound: u8,
    adjust: FirstCharAdjust,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let done = ctx.next_label(&format!("{}_done", label_prefix));
            ctx.emitter.instruction(&format!("cbz x2, {}", done));              // leave empty strings unchanged because there is no first byte
            ctx.emitter.instruction("ldrb w9, [x1]");                           // load the first byte of the copied string for ASCII case checks
            ctx.emitter.instruction(&format!("cmp w9, #{}", lower_bound));      // compare the first byte against the lower ASCII case bound
            ctx.emitter.instruction(&format!("b.lt {}", done));                 // leave bytes below the case range unchanged
            ctx.emitter.instruction(&format!("cmp w9, #{}", upper_bound));      // compare the first byte against the upper ASCII case bound
            ctx.emitter.instruction(&format!("b.gt {}", done));                 // leave bytes above the case range unchanged
            match adjust {
                FirstCharAdjust::Uppercase => {
                    ctx.emitter.instruction("sub w9, w9, #32");                 // convert lowercase ASCII to uppercase
                }
                FirstCharAdjust::Lowercase => {
                    ctx.emitter.instruction("add w9, w9, #32");                 // convert uppercase ASCII to lowercase
                }
            }
            ctx.emitter.instruction("strb w9, [x1]");                           // store the adjusted first byte into the copied string
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let done = ctx.next_label(&format!("{}_done", label_prefix));
            ctx.emitter.instruction("test rdx, rdx");                           // leave empty strings unchanged because there is no first byte
            ctx.emitter.instruction(&format!("jz {}", done));                   // skip first-byte mutation for empty strings
            ctx.emitter.instruction("movzx ecx, BYTE PTR [rax]");               // load the first byte of the copied string for ASCII case checks
            ctx.emitter.instruction(&format!("cmp cl, {}", lower_bound));       // compare the first byte against the lower ASCII case bound
            ctx.emitter.instruction(&format!("jb {}", done));                   // leave bytes below the case range unchanged
            ctx.emitter.instruction(&format!("cmp cl, {}", upper_bound));       // compare the first byte against the upper ASCII case bound
            ctx.emitter.instruction(&format!("ja {}", done));                   // leave bytes above the case range unchanged
            match adjust {
                FirstCharAdjust::Uppercase => {
                    ctx.emitter.instruction("sub cl, 32");                      // convert lowercase ASCII to uppercase
                }
                FirstCharAdjust::Lowercase => {
                    ctx.emitter.instruction("add cl, 32");                      // convert uppercase ASCII to lowercase
                }
            }
            ctx.emitter.instruction("mov BYTE PTR [rax], cl");                  // store the adjusted first byte into the copied string
            ctx.emitter.label(&done);
        }
    }
}

/// Pushes the explicit or default decimal-count argument.
fn push_decimal_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
fn push_separator_byte(
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
fn load_separator_byte(
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
fn emit_aarch64_empty_separator_guard(ctx: &mut FunctionContext<'_>) {
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
fn emit_x86_64_empty_separator_guard(ctx: &mut FunctionContext<'_>) {
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
fn pop_number_format_args(ctx: &mut FunctionContext<'_>) {
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
fn load_as_float(ctx: &mut FunctionContext<'_>, value: ValueId, name: &str) -> Result<()> {
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

/// Loads a concrete scalar value as an integer runtime argument.
fn load_as_int(ctx: &mut FunctionContext<'_>, value: ValueId, name: &str) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
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
