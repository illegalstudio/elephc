//! Purpose:
//! File writes and PHAR compression or metadata bridge helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `file_put_contents(path, data, flags = 0)` through the target-aware runtime writer.
///
/// The optional `$flags` reaches `__rt_file_put_contents_flagged`, which honours `FILE_APPEND`
/// and `LOCK_EX`. Without it the call keeps the original two-argument entry point, so every
/// existing write lowers to exactly the instructions it did before (issue #506).
///
/// A phar destination rejects `$flags` rather than dropping it: a phar write replaces one entry
/// in an archive that is rebuilt wholesale, so there is nothing for `FILE_APPEND` to append to
/// and no descriptor for `LOCK_EX` to lock. Silently ignoring an append would corrupt data.
pub(crate) fn lower_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "file_put_contents", 2, 3)?;
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    let flags = if inst.operands.len() > 2 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    let path_literal = optional_const_string_operand(ctx, path)?;
    if let Some(path_literal) = path_literal.as_deref() {
        if path_literal.starts_with("phar://") {
            if flags.is_some() {
                return Err(CodegenIrError::unsupported(
                    "file_put_contents() $flags on a phar:// destination (a phar entry is \
                     rewritten whole, so FILE_APPEND and LOCK_EX have nothing to act on)"
                        .to_string(),
                ));
            }
            return lower_literal_phar_file_put_contents(ctx, inst, path_literal, data);
        }
    }
    let helper = match (path_literal.is_none(), flags.is_some()) {
        (true, true) => {
            publish_dynamic_phar_write_function_pointer(ctx);
            "__rt_file_put_contents_maybe_phar_flagged"
        }
        (true, false) => {
            publish_dynamic_phar_write_function_pointer(ctx);
            "__rt_file_put_contents_maybe_phar"
        }
        (false, true) => "__rt_file_put_contents_flagged",
        (false, false) => "__rt_file_put_contents",
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_file_put_contents_arm64(ctx, path, data, flags, helper)?,
        Arch::X86_64 => lower_file_put_contents_x86_64(ctx, path, data, flags, helper)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers one-shot `file_put_contents("phar://archive/entry", data)`.
pub(super) fn lower_literal_phar_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
    data: ValueId,
) -> Result<()> {
    if !emit_phar_write_open_for_literal(ctx, path)? {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unresolved phar write target returns failure
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unresolved phar write target returns failure
            }
        }
        return store_if_result(ctx, inst);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, data, "file_put_contents phar data")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_append");
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x9");                              // pass the PHAR write descriptor to finalize
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, data, "file_put_contents phar data")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass the entry payload pointer to the phar writer
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_append");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the compiler-internal native PHAR compression-control helper.
pub(crate) fn lower_elephc_phar_set_compression(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_phar_set_compression", 2)?;
    let path = expect_operand(inst, 0)?;
    let compression = expect_operand(inst, 1)?;
    let fail = ctx.next_label("phar_set_compression_fail");
    let done = ctx.next_label("phar_set_compression_done");
    publish_phar_set_compression_function_pointer(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_result(compression)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, path, "__elephc_phar_set_compression path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_phar_set_compression_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR compression bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge makes compression control fail
            ctx.emitter.instruction("blr x9");                                  // rewrite native-PHAR entry compression flags
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize bridge result to PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.load_value_to_result(compression)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, path, "__elephc_phar_set_compression path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                "r10",
                "_elephc_phar_set_compression_fn",
                0,
            );
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR compression bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge makes compression control fail
            ctx.emitter.instruction("call r10");                                // rewrite native-PHAR entry compression flags
            ctx.emitter.instruction("test rax, rax");                           // test the bridge success flag
            ctx.emitter.instruction("setne al");                                // normalize bridge result to PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_phar_get_metadata()` into the metadata-read bridge call.
pub(crate) fn lower_elephc_phar_get_metadata(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_metadata_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_get_metadata",
        "_elephc_phar_get_metadata_fn",
    )
}

/// Lowers `__elephc_phar_get_stub()` into the stub-read bridge call.
pub(crate) fn lower_elephc_phar_get_stub(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_stub_function_pointer(ctx);
    emit_phar_get_string_bridge(ctx, inst, "__elephc_phar_get_stub", "_elephc_phar_get_stub_fn")
}

/// Lowers `__elephc_phar_set_metadata()` into the metadata-write bridge call.
pub(crate) fn lower_elephc_phar_set_metadata(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_set_metadata_function_pointer(ctx);
    emit_phar_set_string_bridge(
        ctx,
        inst,
        "__elephc_phar_set_metadata",
        "_elephc_phar_set_metadata_fn",
    )
}

/// Lowers `__elephc_phar_set_stub()` into the stub-write bridge call.
pub(crate) fn lower_elephc_phar_set_stub(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_set_stub_function_pointer(ctx);
    emit_phar_set_string_bridge(ctx, inst, "__elephc_phar_set_stub", "_elephc_phar_set_stub_fn")
}

/// Emits a `(path, data)` string -> bool PHAR bridge call (set metadata/stub).
///
/// Loads the path and data strings into the bridge's `(path_ptr, path_len, data_ptr,
/// data_len)` argument registers, calls the optional bridge pointer in `slot`, and
/// normalizes the result to a PHP bool (false when the bridge is unavailable).
pub(super) fn emit_phar_set_string_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    let fail = ctx.next_label("phar_set_string_fail");
    let done = ctx.next_label("phar_set_string_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, data, "phar set-string data")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, path, "phar set-string path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR write bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge makes the write fail
            ctx.emitter.instruction("blr x9");                                  // rewrite the archive with the new metadata/stub
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize bridge result to PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, data, "phar set-string data")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, path, "phar set-string path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR write bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge makes the write fail
            ctx.emitter.instruction("call r10");                                // rewrite the archive with the new metadata/stub
            ctx.emitter.instruction("test rax, rax");                           // test the bridge success flag
            ctx.emitter.instruction("setne al");                                // normalize bridge result to PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a `(string) -> bool` PHAR bridge call (e.g. set the ZipCrypto password).
///
/// Loads the single string argument as (pointer, length), calls the optional bridge
/// pointer in `slot`, and normalizes its return to a PHP bool. A null bridge yields
/// false.
pub(super) fn emit_phar_string_to_bool_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let fail = ctx.next_label("phar_string_bool_fail");
    let done = ctx.next_label("phar_string_bool_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, value, "phar string->bool arg")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = string pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = string length
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge yields false
            ctx.emitter.instruction("blr x9");                                  // call the bridge setter
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge return flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize to a PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, value, "phar string->bool arg")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = string pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = string length
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge yields false
            ctx.emitter.instruction("call r10");                                // call the bridge setter
            ctx.emitter.instruction("test rax, rax");                           // test the bridge return flag
            ctx.emitter.instruction("setne al");                                // normalize to a PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a `(path) -> string` PHAR bridge call (read metadata/stub).
///
/// Calls the optional bridge pointer in `slot` with the path and an out-length slot,
/// then persists the returned bytes into an owned PHP string. A null bridge or a null
/// result yields an owned empty string (the OOP layer treats that as "not set").
pub(super) fn emit_phar_get_string_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    let empty = ctx.next_label("phar_get_string_empty");
    let persist = ctx.next_label("phar_get_string_persist");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, path, "phar get-string path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_symbol_address(ctx.emitter, "x2", "_phar_list_len");      // bridge arg 2 = out-length slot
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR read bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", empty));             // missing bridge yields an empty string
            ctx.emitter.instruction("blr x9");                                  // read the metadata/stub bytes into the global buffer
            ctx.emitter.instruction(&format!("cbz x0, {}", empty));             // a null result means the field is unset
            ctx.emitter.instruction("mov x1, x0");                              // str_persist source pointer = bridge buffer
            abi::emit_symbol_address(ctx.emitter, "x9", "_phar_list_len");
            ctx.emitter.instruction("ldr x2, [x9]");                            // str_persist length = bridge out-length
            ctx.emitter.instruction(&format!("b {}", persist));                 // persist the returned bytes
            ctx.emitter.label(&empty);
            ctx.emitter.instruction("mov x1, #0");                              // empty source pointer (length 0 is not dereferenced)
            ctx.emitter.instruction("mov x2, #0");                              // empty string length
            ctx.emitter.label(&persist);
            ctx.emitter.instruction("bl __rt_str_persist");                     // copy into an owned heap string -> x1=ptr, x2=len
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, path, "phar get-string path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_symbol_address(ctx.emitter, "rdx", "_phar_list_len");     // bridge arg 2 = out-length slot
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR read bridge was published
            ctx.emitter.instruction(&format!("jz {}", empty));                  // missing bridge yields an empty string
            ctx.emitter.instruction("call r10");                                // read the metadata/stub bytes into the global buffer
            ctx.emitter.instruction("test rax, rax");                           // a null result means the field is unset
            ctx.emitter.instruction(&format!("jz {}", empty));                  // fall back to an empty string
            ctx.emitter.instruction("mov rdi, rax");                            // str_persist source pointer = bridge buffer
            abi::emit_load_symbol_to_reg(ctx.emitter, "rdx", "_phar_list_len", 0); // str_persist length = bridge out-length
            ctx.emitter.instruction(&format!("jmp {}", persist));               // persist the returned bytes
            ctx.emitter.label(&empty);
            ctx.emitter.instruction("mov rdi, 0");                              // empty source pointer (length 0 is not dereferenced)
            ctx.emitter.instruction("mov rdx, 0");                              // empty string length
            ctx.emitter.label(&persist);
            ctx.emitter.instruction("call __rt_str_persist");                   // copy into an owned heap string -> rax=ptr, rdx=len
        }
    }
    store_if_result(ctx, inst)
}

