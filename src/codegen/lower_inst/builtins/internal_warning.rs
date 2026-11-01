//! Purpose:
//! Lowers `__elephc_internal_warning($message)`: one php-level `E_WARNING` raised from an
//! elephc-PHP prelude.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_14`.
//!
//! Key details:
//! - The operand is the complete diagnostic line (`"Warning: f(): ...\n"`). It is handed to
//!   `__rt_diag_warning` in the suppressible-diagnostic fragment registers, the same entry the
//!   compiler's own runtime warnings use, so `@`, `error_reporting()`, the user error handler and
//!   `error_get_last()` all see it.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::store_if_result;

/// Loads the message into the diagnostic fragment registers and raises it.
pub(crate) fn lower_elephc_internal_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let message = super::super::expect_operand(inst, 0)?;
    ctx.load_value_to_result(message)?;
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (fragment_ptr, fragment_len) =
        super::super::runtime_class_messages::diagnostic_fragment_regs(ctx.emitter);
    if fragment_ptr != text_ptr {
        ctx.emitter
            .instruction(&format!("mov {}, {}", fragment_ptr, text_ptr));       // the message bytes are the diagnostic fragment
    }
    if fragment_len != text_len {
        ctx.emitter
            .instruction(&format!("mov {}, {}", fragment_len, text_len));       // and so is its length
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    store_if_result(ctx, inst)
}
