//! Purpose:
//! Lowers the two internal TLS PSK setup builtins used by the PHP 8.6
//! stream-context prelude: `__elephc_tls_adapter_addr()` and
//! `__elephc_tls_set_psk_client($descriptor, $adapter)`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_14`.
//!
//! Key details:
//! - The adapter address is the shared callable adapter
//!   `__rt_curl_invoke_callback`; this lowering references a local runtime label,
//!   not a curl or TLS bridge symbol, so the runtime stays bridge-neutral.
//! - The setter stores the normalized descriptor and adapter address in the
//!   process-wide TLS slots read by the `https://` runtime helper. Both pointers
//!   are borrowed from the PHP-rooted callable descriptor and never freed here.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::{expect_operand, store_if_result};

/// Lowers `__elephc_tls_adapter_addr()` to the shared callable adapter address.
pub(crate) fn lower_tls_adapter_addr(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count(inst, "__elephc_tls_adapter_addr", 0)?;
    abi::emit_extern_symbol_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "__rt_curl_invoke_callback",
    );
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_tls_set_psk_client($descriptor, $adapter)` into the TLS slots.
pub(crate) fn lower_tls_set_psk_client(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count(inst, "__elephc_tls_set_psk_client", 2)?;
    let descriptor = expect_operand(inst, 0)?;
    let adapter = expect_operand(inst, 1)?;
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(descriptor, result)?;
    abi::emit_push_reg(ctx.emitter, result);
    ctx.load_value_to_reg(adapter, result)?;
    abi::emit_store_reg_to_symbol(ctx.emitter, result, "_tls_psk_client_adapter", 0);
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_store_reg_to_symbol(ctx.emitter, result, "_tls_psk_client_descriptor", 0);
    abi::emit_load_int_immediate(ctx.emitter, result, 0x7fff_ffff_ffff_fffe);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_tls_mark_early_data($stream)` by copying the last HTTPS
/// early-data flag onto the supplied stream descriptor.
pub(crate) fn lower_tls_mark_early_data(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count(inst, "__elephc_tls_mark_early_data", 1)?;
    let stream = expect_operand(inst, 0)?;
    crate::codegen::lower_inst::builtins::io::load_stream_fd_to_result(
        ctx,
        stream,
        "__elephc_tls_mark_early_data",
    )?;
    let fd = abi::int_result_reg(ctx.emitter);
    let skip = "__rt_tls_mark_early_data_done";
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_https_early_data_attempted", 0);
            ctx.emitter.instruction(&format!("cbz x9, {skip}"));                // no 0-RTT attempt: leave the stream unmarked
            ctx.emitter.instruction(&format!(
                "mov x10, #{}",
                crate::codegen_support::runtime::FD_TABLE_SLOTS
            ));                                                                 // fd table capacity
            ctx.emitter.instruction(&format!("cmp {fd}, x10"));                // is the stream fd inside the flag table?
            ctx.emitter.instruction(&format!("b.hs {skip}"));                  // out of range: leave it unmarked
            abi::emit_load_symbol_to_reg(ctx.emitter, "x11", "_https_early_data_status", 0);
            abi::emit_symbol_address(ctx.emitter, "x12", "_https_early_data_flags");
            ctx.emitter.instruction(&format!("strb w11, [x12, {fd}]"));        // publish the early-data status for this stream
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_https_early_data_attempted", 0);
            ctx.emitter.instruction(&format!("test r10, r10"));                // no 0-RTT attempt: leave the stream unmarked
            ctx.emitter.instruction(&format!("jz {skip}"));                    // skip the status publication
            ctx.emitter.instruction(&format!(
                "mov r11, {}",
                crate::codegen_support::runtime::FD_TABLE_SLOTS
            ));                                                                 // fd table capacity
            ctx.emitter.instruction(&format!("cmp {fd}, r11"));                // is the stream fd inside the flag table?
            ctx.emitter.instruction(&format!("jae {skip}"));                   // out of range: leave it unmarked
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_https_early_data_status", 0);
            abi::emit_symbol_address(ctx.emitter, "r10", "_https_early_data_flags");
            ctx.emitter.instruction(&format!("mov BYTE PTR [r10 + {fd}], r11b")); // publish the early-data status for this stream
        }
    }
    ctx.emitter.label(skip);
    abi::emit_load_int_immediate(ctx.emitter, fd, 0x7fff_ffff_ffff_fffe);
    store_if_result(ctx, inst)
}

/// Rejects an unexpected operand count with a codegen IR error.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(crate::codegen::CodegenIrError::invalid_module(format!(
        "{name} expected {expected} args, got {}",
        inst.operands.len()
    )))
}
