//! Purpose:
//! Lowers AOT `get_resources()` filtering into the shared runtime inventory.
//!
//! Called from:
//! - `super::lower_core_builtin()` for `CoreBuiltinOp::GetResources`.
//!
//! Key details:
//! - Null selects the complete inventory, while strings map to exact PHP type selectors.
//! - Dynamic Mixed null is distinguished before ordinary PHP string coercion.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::expect_operand;

const INVALID_RESOURCE_TYPE_MESSAGE: &str =
    "get_resources(): Argument #1 ($type) must be a valid resource type";

/// Materializes the filtered integer-keyed resource hash for one Core call.
pub(super) fn lower_get_resources(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let filter = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(filter)?.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), -1);
        }
        PhpType::Mixed | PhpType::Union(_) => lower_dynamic_filter(ctx, filter)?,
        _ => {
            crate::codegen::lower_inst::builtins::io::load_string_to_result(
                ctx,
                filter,
                "get_resources type",
            )?;
            abi::emit_call_label(ctx.emitter, "__rt_resource_type_selector");
        }
    }
    emit_invalid_selector_guard(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_get_resources");
    Ok(())
}

/// Raises PHP's catchable `ValueError` when the runtime selector rejected the type name.
fn emit_invalid_selector_guard(ctx: &mut FunctionContext<'_>) {
    let valid = ctx.next_label("get_resources_filter_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // selector 4 is the invalid resource-type sentinel
            ctx.emitter.instruction(&format!("b.ne {valid}"));                  // continue when the runtime recognized the filter
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // selector 4 is the invalid resource-type sentinel
            ctx.emitter.instruction(&format!("jne {valid}"));                   // continue when the runtime recognized the filter
        }
    }
    super::super::exceptions::emit_value_error(ctx, INVALID_RESOURCE_TYPE_MESSAGE);
    ctx.emitter.label(&valid);
}

/// Resolves a boxed nullable filter without stringifying PHP null to an invalid empty name.
fn lower_dynamic_filter(ctx: &mut FunctionContext<'_>, filter: crate::ir::ValueId) -> Result<()> {
    let null = ctx.next_label("get_resources_filter_null");
    let done = ctx.next_label("get_resources_filter_done");
    ctx.load_value_to_result(filter)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // runtime tag 8 is PHP null
            ctx.emitter.instruction(&format!("b.eq {null}"));                   // null requests the complete resource inventory
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_resource_type_selector");
            ctx.emitter.instruction(&format!("b {done}"));                      // skip the null selector after string matching
            ctx.emitter.label(&null);
            abi::emit_pop_reg(ctx.emitter, "x9");                             // discard the preserved Mixed box
            abi::emit_load_int_immediate(ctx.emitter, "x0", -1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // runtime tag 8 is PHP null
            ctx.emitter.instruction(&format!("je {null}"));                     // null requests the complete resource inventory
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_resource_type_selector");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip the null selector after string matching
            ctx.emitter.label(&null);
            abi::emit_pop_reg(ctx.emitter, "r10");                            // discard the preserved Mixed box
            abi::emit_load_int_immediate(ctx.emitter, "rax", -1);
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}
