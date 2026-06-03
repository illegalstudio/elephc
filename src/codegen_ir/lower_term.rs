//! Purpose:
//! Lowers EIR block terminators into jumps, returns, exits, and future control-flow edges.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - The current increment supports process-entry `return` and explicit unsupported
//!   diagnostics for branch/switch/throw paths that still need Phase 04 lowering.

use crate::ir::{Terminator, ValueId};

use crate::codegen::abi;

use super::context::FunctionContext;
use super::frame;
use super::{CodegenIrError, Result};

/// Lowers one EIR terminator.
pub(super) fn lower_terminator(ctx: &mut FunctionContext<'_>, term: &Terminator) -> Result<()> {
    match term {
        Terminator::Return { value: None } => {
            if ctx.is_main {
                frame::emit_main_epilogue(ctx);
            } else {
                jump_to_function_epilogue(ctx)?;
            }
            Ok(())
        }
        Terminator::Return { value: Some(value) } => {
            ctx.load_value_to_result(*value)?;
            jump_to_function_epilogue(ctx)?;
            Ok(())
        }
        Terminator::Unreachable => Ok(()),
        Terminator::Br { target, args } => {
            ensure_no_block_args(args, "br")?;
            let label = ctx.block_label_for_id(*target)?;
            abi::emit_jump(ctx.emitter, &label);
            Ok(())
        }
        Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        } => {
            ensure_no_block_args(then_args, "cond_br then")?;
            ensure_no_block_args(else_args, "cond_br else")?;
            ctx.load_value_to_result(*cond)?;
            let then_label = ctx.block_label_for_id(*then_target)?;
            let else_label = ctx.block_label_for_id(*else_target)?;
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &then_label);
            abi::emit_jump(ctx.emitter, &else_label);
            Ok(())
        }
        Terminator::Switch { .. } => Err(CodegenIrError::unsupported("switch terminator")),
        Terminator::Throw { .. } => Err(CodegenIrError::unsupported("throw terminator")),
        Terminator::Fatal { .. } => Err(CodegenIrError::unsupported("fatal terminator")),
        Terminator::GeneratorSuspend { .. } => {
            Err(CodegenIrError::unsupported("generator_suspend terminator"))
        }
    }
}

/// Emits a jump to the current user function's shared epilogue.
fn jump_to_function_epilogue(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let Some(label) = ctx.epilogue_label.clone() else {
        return Err(CodegenIrError::unsupported(
            "return values on the EIR backend entry function",
        ));
    };
    abi::emit_jump(ctx.emitter, &label);
    Ok(())
}

/// Rejects block arguments until Phase 04 implements block parameter movement.
fn ensure_no_block_args(args: &[ValueId], context: &str) -> Result<()> {
    if args.is_empty() {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} block arguments",
        context
    )))
}
