//! Purpose:
//! Lowers the typed PHP Core runtime and introspection EIR operation family.
//!
//! Called from:
//! - `super::lower_instruction()` for `Op::CoreBuiltin`.
//!
//! Key details:
//! - Results declared as `mixed` are returned as owned boxed runtime cells.
//! - Static compiler metadata is preferred over PHP-name-driven runtime dispatch.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::ir::{CoreBuiltinOp, Immediate, Instruction};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

mod backtrace;
mod constants;
mod handlers;
mod introspection;
mod resources;

/// Publishes source and frame-reader metadata before an instruction can enter PHP code.
pub(super) fn prepare_backtrace_call_site(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    backtrace::prepare_call_site(ctx, inst)
}

/// Lowers one selector-validated PHP Core operation.
pub(super) fn lower_core_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let operation = core_operation(inst)?;
    match operation {
        CoreBuiltinOp::DebugBacktrace => backtrace::lower_debug_backtrace(ctx, inst)?,
        CoreBuiltinOp::DebugPrintBacktrace => {
            backtrace::lower_debug_print_backtrace(ctx, inst)?
        }
        CoreBuiltinOp::ErrorReporting => handlers::lower_error_reporting(ctx, inst)?,
        CoreBuiltinOp::RestoreErrorHandler => handlers::lower_restore_error_handler(ctx)?,
        CoreBuiltinOp::RestoreExceptionHandler => {
            handlers::lower_restore_exception_handler(ctx)?
        }
        CoreBuiltinOp::SetErrorHandler => handlers::lower_set_error_handler(ctx, inst)?,
        CoreBuiltinOp::SetExceptionHandler => handlers::lower_set_exception_handler(ctx, inst)?,
        CoreBuiltinOp::TriggerError => handlers::lower_trigger_error(ctx, inst)?,
        CoreBuiltinOp::GetDefinedConstants => constants::lower_get_defined_constants(ctx, inst)?,
        CoreBuiltinOp::GetDefinedVars => emit_empty_mixed_hash(ctx)?,
        CoreBuiltinOp::GetDefinedFunctions => introspection::lower_get_defined_functions(ctx)?,
        CoreBuiltinOp::GetExtensionFuncs => {
            introspection::lower_get_extension_funcs(ctx, inst)?
        }
        CoreBuiltinOp::GetIncludedFiles => introspection::lower_get_included_files(ctx)?,
        CoreBuiltinOp::GetResources => resources::lower_get_resources(ctx, inst)?,
        CoreBuiltinOp::GetMangledObjectVars => {
            crate::codegen::lower_inst::builtins::types::emit_mangled_object_vars(ctx, inst)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Decodes the typed selector carried by a validated Core operation.
fn core_operation(inst: &Instruction) -> Result<CoreBuiltinOp> {
    let Some(Immediate::I64(selector)) = inst.immediate.as_ref() else {
        return Err(CodegenIrError::invalid_module(
            "core_builtin missing selector immediate",
        ));
    };
    CoreBuiltinOp::from_i64(*selector).ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid core_builtin selector {selector}"))
    })
}

/// Allocates and boxes an empty associative Mixed array.
fn emit_empty_mixed_hash(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 16);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 7);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 16);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 7);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    Ok(())
}

/// Boxes PHP null as a fresh Mixed result.
fn emit_null_mixed_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Emits an integer-backed PHP boolean result.
fn emit_bool_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}
