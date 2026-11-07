//! Purpose:
//! Lowers the internal `__elephc_debug_event` builtin to the native event
//! recorder exported by `elephc-debug`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_08` when the typed
//!   runtime target is `ElephcDebugEvent`.
//!
//! Key details:
//! - The six-argument call is `(kind, mixed value, label ptr/len, to ptr/len)`;
//!   it fits the integer argument registers on both supported architectures.
//! - The mixed value is already boxed by standard builtin argument lowering; the
//!   recorder unboxes it with the runtime's boxed-cell contract.
//! - Only `--debug` builds link the recorder, so other builds fail at codegen
//!   rather than at link time.

use crate::codegen::abi;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::expect_operand;

/// Lowers one dump or trace-segment capture event call.
pub(crate) fn lower_event(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !ctx.shared.debug_build {
        return Err(CodegenIrError::unsupported(
            "elephc_dump/elephc_trace_start/elephc_trace_end require --debug",
        ));
    }
    let kind = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let label = inst.operands.get(2).copied();
    let to = inst.operands.get(3).copied();

    let target = ctx.emitter.target;
    let arg = |index| abi::int_arg_reg_name(target, index);
    // Box the value first: standard builtin argument lowering leaves a `mixed`
    // parameter in its source representation, and boxing may clobber the integer
    // argument registers. The boxed cell is staged on the temporary stack while
    // the remaining arguments are materialized.
    let value_type = ctx.load_value_to_result(value)?.codegen_repr();
    let box_type = ctx.mixed_box_type(value)?;
    let releases_box = !matches!(value_type, PhpType::Mixed | PhpType::Union(_));
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &box_type);
    let result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result);

    ctx.load_value_to_reg(kind, arg(0))?;
    load_optional_string(ctx, label, arg(2), arg(3))?;
    load_string(ctx, to, arg(4), arg(5))?;
    abi::emit_pop_reg(ctx.emitter, arg(1));
    if releases_box {
        abi::emit_push_reg(ctx.emitter, arg(1));
    }
    abi::emit_call_label(ctx.emitter, "elephc_debug_event");
    if releases_box {
        abi::emit_pop_reg(ctx.emitter, &result);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    Ok(())
}

/// Loads an optional string pointer/length pair, or two zero words for null.
fn load_optional_string(
    ctx: &mut FunctionContext<'_>,
    value: Option<ValueId>,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(value) = value else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => ctx.load_string_value_to_regs(value, ptr_reg, len_reg),
        PhpType::Void | PhpType::Never | PhpType::TaggedScalar => {
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "debug event label is {other:?}, expected string or null"
        ))),
    }
}

/// Loads a required string pointer/length pair.
fn load_string(
    ctx: &mut FunctionContext<'_>,
    value: Option<ValueId>,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let Some(value) = value else {
        abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        return Ok(());
    };
    if ctx.value_php_type(value)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(
            "debug event destination is not a string",
        ));
    }
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)
}
