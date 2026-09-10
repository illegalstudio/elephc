//! Purpose:
//! Defines the internal compiled-PHP return ownership protocol and EIR provenance rules.
//!
//! Called from:
//! - `crate::codegen::lower_term`, direct-call lowering, frame epilogues, and native invokers.
//!
//! Key details:
//! - AArch64 uses `x15`; x86_64 uses `r11`. This sideband is private to compiled PHP calls.
//! - Each callee publishes the exact return-path state after cleanup. A `MaybeOwned` direct-call
//!   result spills it immediately, before later calls can clobber the caller-saved register.
//! - Lifetime-tracked block parameters receive one owner on every incoming edge, so joins carry
//!   a stable owned convention instead of losing heterogeneous path provenance.
//! - The marker is defined only for non-string by-value lifetime-tracked results and is caller-saved.
//!   Typed strings use their dedicated persist and ownership-transfer boxing path instead.
//! - Native C callers never see the marker. Their invoker consumes it and returns one owned cell.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{LocalSlotId, Op, Ownership, ValueDef, ValueId};
use crate::types::PhpType;

/// Ownership state published with one by-value compiled PHP return.
#[derive(Clone, Copy)]
pub(super) enum ReturnOwnershipStatus {
    /// Final EIR metadata and frame analysis prove the return-path state.
    Static(bool),
    /// A nested PHP call published the path-specific state stored at this frame offset.
    Dynamic(usize),
}

/// Reconciles an optimized physical value shape with the declared PHP return ABI.
///
/// EIR optimization may replace a boxed Mixed producer with a raw scalar or
/// concrete container. A callable entry still promises the declared Mixed ABI,
/// so this boxes the narrowed value and transfers or retains its source owner
/// according to the same path-specific provenance.
pub(super) fn normalize_loaded_return_representation(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    source_ownership: ReturnOwnershipStatus,
) -> ReturnOwnershipStatus {
    let return_ty = ctx.function.return_php_type.codegen_repr();
    if !matches!(return_ty, PhpType::Mixed | PhpType::Union(_))
        || matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        return source_ownership;
    }
    match source_ownership {
        ReturnOwnershipStatus::Static(true) => {
            crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, source_ty);
        }
        ReturnOwnershipStatus::Static(false) => {
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, source_ty);
        }
        ReturnOwnershipStatus::Dynamic(offset) => {
            let owned = ctx.next_label("return_box_source_owned");
            let done = ctx.next_label("return_box_source_done");
            emit_load_status(ctx.emitter, offset);
            emit_branch_if_owned(ctx.emitter, &owned);
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, source_ty);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&owned);
            crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, source_ty);
            ctx.emitter.label(&done);
        }
    }
    ReturnOwnershipStatus::Static(true)
}

/// Resolves one return path to static ownership or a preserved nested-call marker.
pub(super) fn classify_return_value(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    skip_return_slot: Option<LocalSlotId>,
) -> Result<ReturnOwnershipStatus> {
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    if skip_return_slot.is_some() {
        return Ok(ReturnOwnershipStatus::Static(true));
    }
    if !Ownership::php_type_needs_lifetime_tracking(&metadata.php_type.codegen_repr()) {
        return Ok(ReturnOwnershipStatus::Static(false));
    }
    match metadata.ownership {
        Ownership::Owned => return Ok(ReturnOwnershipStatus::Static(true)),
        Ownership::Borrowed | Ownership::Persistent | Ownership::NonHeap => {
            return Ok(ReturnOwnershipStatus::Static(false));
        }
        Ownership::Moved => {
            return Err(CodegenIrError::invalid_module(format!(
                "return uses moved value {}",
                value.as_raw()
            )));
        }
        Ownership::MaybeOwned => {}
    }
    if metadata.php_type.codegen_repr() == PhpType::Str {
        // String return lowering and the native invoker already persist their
        // scratch or borrowed storage before it crosses either boundary.
        return Ok(ReturnOwnershipStatus::Static(false));
    }
    if let Some(offset) = ctx.runtime_return_ownership_offset(value) {
        return Ok(ReturnOwnershipStatus::Dynamic(offset));
    }
    match metadata.def {
        ValueDef::BlockParam { .. } => {
            // Lifetime-tracked block arguments are normalized to one owner on
            // every incoming edge before this parameter slot is written.
            Ok(ReturnOwnershipStatus::Static(true))
        }
        ValueDef::Instruction { inst, .. } => {
            let instruction = ctx
                .function
                .instruction(inst)
                .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
            match instruction.op {
                Op::LoadLocal | Op::LoadRefCell | Op::LoadGlobal | Op::LoadStaticLocal => {
                    Ok(ReturnOwnershipStatus::Static(false))
                }
                Op::Move | Op::Borrow => {
                    let source = instruction.operands.first().copied().ok_or_else(|| {
                        CodegenIrError::invalid_module(format!(
                            "{:?} value {} has no source operand",
                            instruction.op,
                            value.as_raw()
                        ))
                    })?;
                    classify_return_value(ctx, source, None)
                }
                _ => Err(CodegenIrError::invalid_module(format!(
                    "ambiguous ownership for returned {} value {} produced by {:?}",
                    metadata.php_type,
                    value.as_raw(),
                    instruction.op
                ))),
            }
        }
    }
}

/// Normalizes one lifetime-tracked block parameter to an owner on its incoming edge.
pub(super) fn normalize_loaded_block_argument(
    ctx: &mut FunctionContext<'_>,
    param: ValueId,
    arg: ValueId,
    ty: &PhpType,
) -> Result<()> {
    let param_metadata = ctx
        .function
        .value(param)
        .ok_or_else(|| CodegenIrError::missing_entry("value", param.as_raw()))?;
    if param_metadata.ownership != Ownership::MaybeOwned
        || !Ownership::php_type_needs_lifetime_tracking(&param_metadata.php_type.codegen_repr())
    {
        return Ok(());
    }
    match classify_return_value(ctx, arg, None)? {
        ReturnOwnershipStatus::Static(true)
            if ctx.value_can_transfer_ownership_to_consumer(arg)? => Ok(()),
        ReturnOwnershipStatus::Static(true) => acquire_loaded_block_argument(ctx, ty),
        ReturnOwnershipStatus::Static(false) => acquire_loaded_block_argument(ctx, ty),
        ReturnOwnershipStatus::Dynamic(offset) => {
            let owned = ctx.next_label("block_arg_owned");
            emit_load_status(ctx.emitter, offset);
            emit_branch_if_owned(ctx.emitter, &owned);
            acquire_loaded_block_argument(ctx, ty)?;
            ctx.emitter.label(&owned);
            Ok(())
        }
    }
}

/// Gives the currently loaded block argument one independent owner.
fn acquire_loaded_block_argument(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
        PhpType::Callable => {
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Callable);
        }
        PhpType::Buffer(_) => {}
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(ctx.emitter, &other);
        }
        PhpType::Void | PhpType::Never => {}
        other => {
            return Err(CodegenIrError::invalid_module(format!(
                "lifetime-tracked block parameter has non-heap argument type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Publishes whether this exact EIR return path transfers its result owner.
pub(super) fn emit_status(emitter: &mut Emitter, owned: bool) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(if owned {                                      // publish the exact EIR return-path ownership status
                "mov x15, #1"
            } else {
                "mov x15, xzr"
            });
        }
        Arch::X86_64 => {
            emitter.instruction(if owned {                                      // publish the exact EIR return-path ownership status
                "mov r11d, 1"
            } else {
                "xor r11d, r11d"
            });
        }
    }
}

/// Preserves the internal marker before argument cleanup can issue another call.
pub(super) fn emit_store_status(emitter: &mut Emitter, offset: usize) {
    let status_reg = status_reg(emitter.target.arch);
    abi::store_at_offset(emitter, status_reg, offset);
}

/// Restores a previously preserved internal marker.
pub(super) fn emit_load_status(emitter: &mut Emitter, offset: usize) {
    let status_reg = status_reg(emitter.target.arch);
    abi::load_at_offset(emitter, status_reg, offset);
}

/// Branches when the current compiled PHP return transfers an existing owner.
pub(super) fn emit_branch_if_owned(emitter: &mut Emitter, owned_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(                                       // transfer a result whose EIR return path already owns its storage
                "cbnz x15, {}",
                owned_label
            ));
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // inspect the per-return EIR ownership status from the callable entry
            emitter.instruction(&format!("jne {}", owned_label));               // transfer a result whose EIR return path already owns its storage
        }
    }
}

/// Returns the private caller-saved register used by the compiled PHP ABI.
fn status_reg(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "x15",
        Arch::X86_64 => "r11",
    }
}
