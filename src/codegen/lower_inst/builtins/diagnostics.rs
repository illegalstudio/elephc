//! Purpose:
//! Lowers the internal `__elephc_deprecated` builtin, which lets an injected prelude body raise
//! one php `Deprecated:` diagnostic in its own name.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` through `RuntimeFnId::ElephcDeprecated`.
//!
//! Key details:
//! - `__rt_diag_warning` reads its message from x1/x2 on AArch64 and rdi/rsi on x86_64 — the
//!   AArch64 pair deliberately skips x0 so a caller can raise a diagnostic without disturbing a
//!   result already in flight. That is why the pair is written per arch here rather than through
//!   the shared first-argument helper.
//! - The whole line, `Deprecated: ` prefix and trailing newline included, comes from the caller.
//!   The helper accumulates pieces and flushes on the newline, so a message that does not end in
//!   one would be held until the next diagnostic — the callers in
//!   `src/types/checker/builtin_spl_classes/filesystem.rs` all pass a complete line.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `__elephc_deprecated($message)` to one `__rt_diag_warning` call.
pub(crate) fn lower_elephc_deprecated(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let message = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(message, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(message, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");                      // stdout, and `@` suppresses it
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);                    // php `void`, never read
    store_if_result(ctx, inst)
}
