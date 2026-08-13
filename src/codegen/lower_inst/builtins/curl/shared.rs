//! Purpose:
//! The register names and the two operand-marshalling steps every curl lowering shares:
//! naming the target's C argument registers, and loading a boxed easy-handle operand into
//! the first of them.
//!
//! Called from:
//! - The `__elephc_curl_*` lowerings in this module.
//!
//! Key details:
//! - THE C ARGUMENT REGISTERS ARE NAMED, NOT HARDCODED PER LOWERING. `curl_arg_reg(ctx, n)`
//!   is the single place either target's calling convention appears, so a lowering reads
//!   as "argument 0, argument 1, ..." and gains its second target for free.
//! - THE HANDLE ALWAYS ARRIVES BOXED. `load_stream_fd_to_result` unboxes a tag-9 Mixed
//!   cell into the integer result register and raises PHP's TypeError for anything else;
//!   the bridge's `i64` handle id then moves from there into the first C argument
//!   register. On AArch64 the result register already IS that register, so the move is
//!   elided by the shared reg-move helper.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;

use super::super::super::expect_operand;
use crate::codegen::context::FunctionContext;

/// Returns the target's `index`-th C integer argument register.
///
/// Only the first FIVE are named. `elephc_curl_easy_str_op` is the widest entry point the
/// runtime reaches, at five arguments; a sixth would collide with the entry-pointer
/// scratch register on x86_64 (`r9`, `codegen_support::runtime::curl::slots::entry_reg`)
/// and would need that scratch moved first. An out-of-range index is a lowering bug rather
/// than a runtime condition, so it panics rather than silently picking a wrong register.
pub(super) fn curl_arg_reg(ctx: &FunctionContext<'_>, index: usize) -> &'static str {
    match (ctx.emitter.target.arch, index) {
        (Arch::AArch64, 0) => "x0",
        (Arch::AArch64, 1) => "x1",
        (Arch::AArch64, 2) => "x2",
        (Arch::AArch64, 3) => "x3",
        (Arch::AArch64, 4) => "x4",
        (Arch::X86_64, 0) => "rdi",
        (Arch::X86_64, 1) => "rsi",
        (Arch::X86_64, 2) => "rdx",
        (Arch::X86_64, 3) => "rcx",
        (Arch::X86_64, 4) => "r8",
        (_, index) => panic!("curl lowering requested C argument register {index}"),
    }
}

/// Loads the boxed easy-handle operand at `index` into the first C argument register.
///
/// The unboxing itself lands in the integer result register (that is what
/// `load_stream_fd_to_result` guarantees), so this moves it into place afterwards rather
/// than assuming the two coincide.
pub(super) fn load_handle_to_first_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<()> {
    let handle = expect_operand(inst, index)?;
    super::super::io::load_stream_fd_to_result(ctx, handle, name)?;
    let dest = curl_arg_reg(ctx, 0);
    let source = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, dest, source);
    Ok(())
}

/// Verifies a curl lowering received exactly `expected` operands.
pub(super) fn ensure_curl_arg_count(
    inst: &Instruction,
    name: &str,
    expected: usize,
) -> Result<()> {
    if inst.operands.len() != expected {
        return Err(CodegenIrError::invalid_module(format!(
            "{name} expected {expected} args, got {}",
            inst.operands.len()
        )));
    }
    Ok(())
}
