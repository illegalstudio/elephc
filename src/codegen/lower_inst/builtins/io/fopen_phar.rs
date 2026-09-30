//! Purpose:
//! Literal PHAR stream reads and writes.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;



/// Emits the boxed result for a literal read-mode `phar://` stream open.
pub(super) fn emit_literal_phar_fopen_read_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match crate::codegen::phar_stream::extract_phar_entry(path) {
        Some(payload) => {
            let (symbol, len) = ctx.data.add_string(&payload);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &symbol);
                    ctx.emitter.instruction(&format!("mov x1, #{}", len));      // embedded phar entry byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &symbol);
                    ctx.emitter.instruction(&format!("mov rsi, {}", len));      // embedded phar entry byte length
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_data_stream");
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unresolved phar entry lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unresolved phar entry lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_phar");
    emit_record_stream_meta_after_boxed_literal(ctx, 5, path);
    Ok(())
}

/// Emits the boxed stream result for a literal write-mode `phar://` stream open.
pub(super) fn emit_literal_phar_fopen_write_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    if !emit_phar_write_open_for_literal(ctx, path)? {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unresolved phar write target lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unresolved phar write target lowers to PHP false
            }
        }
    }
    box_stream_fd_or_false_result(ctx, "fopen_phar_write");
    emit_record_stream_meta_after_boxed_literal(ctx, 5, path);
    Ok(())
}

/// Seeds the PHAR write buffer for a literal target and records the output archive path.
pub(super) fn emit_phar_write_open_for_literal(ctx: &mut FunctionContext<'_>, url: &str) -> Result<bool> {
    let Some((archive, entry)) = crate::codegen::phar_stream::resolve_write_target(url)
    else {
        return Ok(false);
    };
    let template = crate::codegen::phar_stream::build_phar_write_template(&entry);
    let (template_label, template_len) = ctx.data.add_string(&template);
    let (path_label, path_len) = ctx.data.add_string(archive.as_bytes());
    let (entry_label, entry_len) = ctx.data.add_string(entry.as_bytes());
    publish_phar_write_function_pointer(ctx);
    crate::codegen::hash_crypto::publish_elephc_crypto_function_pointers(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", &path_label);
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_path_ptr");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive path pointer for finalize
            ctx.emitter.instruction(&format!("mov x9, #{}", path_len));         // materialize the archive path byte length
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_path_len");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive path length for finalize
            abi::emit_symbol_address(ctx.emitter, "x9", &entry_label);
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_entry_ptr");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive entry name pointer for finalize
            ctx.emitter.instruction(&format!("mov x9, #{}", entry_len));        // materialize the archive entry name byte length
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_entry_len");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive entry name length for finalize
            abi::emit_symbol_address(ctx.emitter, "x0", &template_label);
            ctx.emitter.instruction(&format!("mov x1, #{}", template_len));     // pass the single-entry PHAR template length
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_open");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", &path_label);
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_path_ptr");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // record the archive path pointer for finalize
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_path_len");
            ctx.emitter.instruction(
                &format!("mov QWORD PTR [r10], {}", path_len)
            );                                                                  // record the archive path length for finalize
            abi::emit_symbol_address(ctx.emitter, "r9", &entry_label);
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_entry_ptr");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // record the archive entry name pointer for finalize
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_entry_len");
            ctx.emitter.instruction(
                &format!("mov QWORD PTR [r10], {}", entry_len)
            );                                                                  // record the archive entry name length for finalize
            abi::emit_symbol_address(ctx.emitter, "rdi", &template_label);
            ctx.emitter.instruction(&format!("mov rsi, {}", template_len));     // pass the single-entry PHAR template length
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_open");
        }
    }
    Ok(true)
}

/// Returns true when a literal fopen mode opens a PHAR entry for writing.
pub(super) fn literal_fopen_mode_is_write(ctx: &FunctionContext<'_>, mode: ValueId) -> Result<bool> {
    Ok(optional_const_string_operand(ctx, mode)?
        .and_then(|mode| mode.as_bytes().first().copied())
        .is_some_and(|first| matches!(first, b'w' | b'a' | b'c' | b'x')))
}

