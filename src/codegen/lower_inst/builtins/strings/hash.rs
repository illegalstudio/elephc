//! Purpose:
//! Lowers hash, HMAC, streaming hash, CRC32, and fixed digest builtins.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Crypto bridge pointers and per-target argument layouts are published before runtime calls.

use super::*;

/// Lowers `hash(algo, data, binary?)` through the shared runtime digest dispatcher.
pub(crate) fn lower_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    store_if_result(ctx, inst)
}

/// Lowers `hash_hmac(algo, data, key, binary?)` through the shared HMAC runtime dispatcher.
pub(crate) fn lower_hash_hmac(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_hmac");
    store_if_result(ctx, inst)
}

/// Lowers `hash_equals(known, user)` through the timing-safe runtime compare helper.
pub(crate) fn lower_hash_equals(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_binary_string_args(ctx, inst, "hash_equals")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_equals");
    store_if_result(ctx, inst)
}

/// Lowers `hash_algos()` through the runtime algorithm-list builder.
pub(crate) fn lower_hash_algos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(crate) fn lower_hash_init(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "hash_init", 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "hash_init", string_ptr_reg(ctx), string_len_reg(ctx))?;
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_init");
    store_if_result(ctx, inst)
}

/// Lowers `hash_update(context, data)` through the incremental hash runtime helper.
pub(crate) fn lower_hash_update(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "hash_update", 2)?;
    let context = expect_operand(inst, 0)?;
    super::io::load_resource_payload_to_result(ctx, context, "hash_update")?;
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
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_update");
    store_if_result(ctx, inst)
}

/// Lowers `hash_final(context, binary?)` through the incremental hash finalizer.
pub(crate) fn lower_hash_final(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "hash_final expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let context = expect_operand(inst, 0)?;
    super::io::load_resource_payload_to_result(ctx, context, "hash_final")?;
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
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_final");
    store_if_result(ctx, inst)
}

/// Lowers `hash_copy(context)` through the incremental hash clone helper.
pub(crate) fn lower_hash_copy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "hash_copy", 1)?;
    let context = expect_operand(inst, 0)?;
    super::io::load_resource_payload_to_result(ctx, context, "hash_copy")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the hash context handle to the C ABI helper
    }
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_copy");
    store_if_result(ctx, inst)
}

/// Lowers `crc32(string)` through the shared checksum runtime helper.
pub(crate) fn lower_crc32(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "crc32")?;
    abi::emit_call_label(ctx.emitter, "__rt_crc32");
    store_if_result(ctx, inst)
}

/// Lowers `mb_strlen(string, encoding = null)` through the multibyte runtime helper.
///
/// Omitted/null encodings use a null pointer plus zero length; explicit names stay byte strings for PHP-compatible case-insensitive lookup and `ValueError` handling.
pub(crate) fn lower_mb_strlen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "mb_strlen", 1, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "mb_strlen", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the source string while loading the optional encoding
            load_optional_mb_strlen_encoding(ctx, inst, "x3", "x4")?;
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the source string for the runtime helper
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "mb_strlen", "rax", "rdx")?;
            ctx.emitter.instruction("push rax");                                // preserve the source string pointer while loading the optional encoding
            ctx.emitter.instruction("push rdx");                                // preserve the source string length while loading the optional encoding
            load_optional_mb_strlen_encoding(ctx, inst, "r8", "r9")?;
            ctx.emitter.instruction("pop rdx");                                 // restore the source string length for the runtime helper
            ctx.emitter.instruction("pop rax");                                 // restore the source string pointer for the runtime helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mb_strlen");
    store_if_result(ctx, inst)
}

/// Loads the nullable optional `mb_strlen()` encoding into a pointer/length pair.
pub(super) fn load_optional_mb_strlen_encoding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(encoding) = inst.operands.get(1).copied() else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    if matches!(ctx.value_php_type(encoding)?, PhpType::Void | PhpType::Never) {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    }
    load_value_as_string_to_regs(ctx, encoding, "mb_strlen encoding", ptr_reg, len_reg)
}

/// Lowers `md5(data, binary?)` through the shared crypto-backed runtime helper.
pub(crate) fn lower_md5(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_fixed_hash(ctx, inst, "md5", "__rt_md5")
}

/// Lowers `sha1(data, binary?)` through the shared crypto-backed runtime helper.
pub(crate) fn lower_sha1(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_fixed_hash(ctx, inst, "sha1", "__rt_sha1")
}

/// Lowers fixed-algorithm hash builtins that share the `__rt_hash` contract.
pub(super) fn lower_fixed_hash(
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
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}
/// Materializes AArch64 `hash()` runtime arguments.
pub(super) fn lower_hash_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_hash_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_hash_hmac_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(super) fn lower_hash_hmac_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
