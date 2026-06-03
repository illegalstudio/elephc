//! Purpose:
//! Lowers EIR block terminators into jumps, returns, exits, and fatal termination paths.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - Fatal terminators write their data-pool diagnostic to stderr and exit.
//! - Throw and generator suspension remain explicit unsupported Phase 04 paths.

use crate::codegen::platform::Arch;
use crate::ir::{DataId, SwitchCase, Terminator, ValueId};

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
        Terminator::Switch {
            scrutinee,
            cases,
            default,
            default_args,
        } => {
            ensure_no_block_args(default_args, "switch default")?;
            lower_switch(ctx, *scrutinee, cases, *default)
        }
        Terminator::Throw { .. } => Err(CodegenIrError::unsupported("throw terminator")),
        Terminator::Fatal { message } => lower_fatal(ctx, *message),
        Terminator::GeneratorSuspend { .. } => {
            Err(CodegenIrError::unsupported("generator_suspend terminator"))
        }
    }
}

/// Lowers an unrecoverable fatal diagnostic and process exit.
fn lower_fatal(ctx: &mut FunctionContext<'_>, message: DataId) -> Result<()> {
    let (message_label, message_len) = ctx.intern_string_data(message)?;
    ctx.emitter.blank();
    ctx.emitter.comment("fatal");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // fd = stderr for the EIR fatal diagnostic
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the EIR fatal diagnostic byte length to write
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // fd = stderr for the EIR fatal diagnostic
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the EIR fatal diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the EIR fatal diagnostic before exiting
        }
    }
    abi::emit_exit(ctx.emitter, 1);
    Ok(())
}

/// Lowers an integer switch by comparing the scrutinee against each case value in source order.
fn lower_switch(
    ctx: &mut FunctionContext<'_>,
    scrutinee: ValueId,
    cases: &[SwitchCase],
    default: crate::ir::BlockId,
) -> Result<()> {
    for case in cases {
        ensure_no_block_args(&case.args, "switch case")?;
    }
    ctx.load_value_to_result(scrutinee)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let case_reg = abi::secondary_scratch_reg(ctx.emitter);
    for case in cases {
        let target_label = ctx.block_label_for_id(case.target)?;
        abi::emit_load_int_immediate(ctx.emitter, case_reg, case.value);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, case_reg)); // compare switch scrutinee with the case value
                ctx.emitter.instruction(&format!("b.eq {}", target_label));     // branch to the matching switch case
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, case_reg)); // compare switch scrutinee with the case value
                ctx.emitter.instruction(&format!("je {}", target_label));       // branch to the matching switch case
            }
        }
    }
    let default_label = ctx.block_label_for_id(default)?;
    abi::emit_jump(ctx.emitter, &default_label);
    Ok(())
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
