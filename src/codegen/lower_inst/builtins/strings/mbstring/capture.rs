//! Purpose:
//! Stages managed local output references and their publication policy for typed mbstring calls.
//!
//! Called from:
//! - Shared mbstring runtime-function lowering for capture and query parsing operations.
//!
//! Key details:
//! - An output pointer denotes the persistent reference wrapper, never its previous PHP value.
//! - Unproven raw, property, parameter, or capture storage fails before machine code executes.
//! - Every accepted local or alias is backed by a tracked Mixed reference owner.

use super::*;
use crate::codegen::lower_inst::local_slot_for_loaded_value;
use crate::ir::LocalSlotId;
use std::collections::HashSet;

/// Stages one proven managed reference using its live local storage rather than its loaded value.
pub(super) fn stage_reference(ctx: &mut FunctionContext<'_>, value: ValueId, pointer: usize) -> Result<()> {
    let slot = local_slot_for_loaded_value(ctx, value)?;
    if !managed_local(ctx, slot, &mut HashSet::new()) {
        return Err(CodegenIrError::unsupported(
            "mbstring output requires a managed Mixed local reference; this lvalue form is not yet supported"));
    }
    let register = abi::int_result_reg(ctx.emitter);
    ctx.materialize_local_storage_address(slot, register)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x0, x0, #8");                          // recover the tracked wrapper from its writable child slot
            ctx.emitter.instruction(&format!("str x0, [sp, #{pointer}]"));      // retain output identity without copying the previous PHP value
        },
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rax, 8");                              // address the tracked wrapper rather than its current child value
            ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {pointer}], rax")); // pass the live persistent reference to the V4 host
        },
    }
    Ok(())
}

/// Creates the reviewed untyped publication state after all other native call inputs are staged.
pub(super) fn stage_state(ctx: &mut FunctionContext<'_>, offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("stp xzr, xzr, [sp, #{offset}]")); // select untyped initialization and clear displaced ownership
            ctx.emitter.instruction(&format!("add x5, sp, #{offset}"));         // pass the native capture state as the sixth C input
        },
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], 0")); // select untyped capture publication
            ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", offset + 8)); // initialize displaced ownership before callbacks
            ctx.emitter.instruction(&format!("lea r9, [rsp + {offset}]"));      // pass capture state without changing the first five C inputs
        },
    }
}

/// Rejects every alias source that could point to an untracked raw cell instead of a managed wrapper.
fn managed_local(ctx: &FunctionContext<'_>, slot: LocalSlotId, visiting: &mut HashSet<LocalSlotId>) -> bool {
    if !visiting.insert(slot) { return false; }
    let Some(local) = ctx.function.locals.get(slot.as_raw() as usize) else { return false; };
    if local.kind != crate::ir::LocalKind::PhpLocal || local.php_type.codegen_repr() != PhpType::Mixed {
        return false;
    }
    if ctx.function.params.iter().any(|param| param.by_ref && local.name.as_deref() == Some(param.name.as_str())) {
        return false;
    }
    let mut backed = false;
    for inst in &ctx.function.instructions {
        if inst.op == Op::BindRefCellPtr && inst.immediate == Some(Immediate::LocalSlot(slot)) {
            return false;
        }
        let Some(Immediate::LocalSlotPair { first, second }) = inst.immediate else { continue; };
        if first != slot { continue; }
        match inst.op {
            Op::PromoteLocalRefCell => {
                if !ctx.function.locals.get(second.as_raw() as usize)
                    .is_some_and(|owner| owner.kind == crate::ir::LocalKind::RefCell && owner.php_type.codegen_repr() == PhpType::Mixed) {
                    return false;
                }
                backed = true;
            },
            Op::AliasLocalRefCell => {
                if !managed_local(ctx, second, visiting) { return false; }
                backed = true;
            },
            _ => {},
        }
    }
    visiting.remove(&slot);
    backed
}
