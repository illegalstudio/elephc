//! Purpose:
//! Routes typed EIR runtime-function IDs to bounded backend implementation groups.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_calls` for typed runtime operations.
//!
//! Key details:
//! - No PHP-name lookup participates in backend dispatch.
//! - Group files keep dispatch bounded while reusing the target-aware emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, RuntimeFnId, ValueId};
use crate::types::PhpType;

mod group_00;
mod group_01;
mod group_02;
mod group_03;
mod group_04;
mod group_05;
mod group_06;
mod group_07;
mod group_08;
mod group_09;
mod group_10;
mod group_11;
mod group_12;
mod group_13;
mod group_14;

/// Whether `target` reads its array operand's storage through the static element type, so a
/// runtime-promoted hash behind a static `array<T>` must be routed to the hash lowering.
fn consumes_possibly_promoted_array(target: RuntimeFnId) -> bool {
    use RuntimeFnId as F;
    matches!(
        target,
        F::InArray | F::ArraySearch | F::ArrayKeyLast | F::Max | F::Min | F::ArraySum
            | F::ArrayProduct | F::Implode | F::ArrayFlip | F::ArrayValues | F::ArrayKeys
            | F::ArrayUnique | F::ArrayCountValues | F::ArrayCombine | F::ArrayDiff
            | F::ArrayDiffKey | F::ArrayDiffAssoc | F::ArrayIntersect | F::ArrayIntersectKey
            | F::ArrayIntersectAssoc | F::ArrayReverse | F::ArrayMap
            | F::ArrayMergeRecursive | F::ArrayReplace | F::ArrayChunk | F::ArraySplice
            | F::Sort | F::Rsort | F::Usort | F::Uasort | F::Uksort | F::Asort | F::Arsort
            | F::Ksort | F::Krsort | F::ArrayFilter | F::ArrayWalk
            | F::ArrayReduce | F::ArrayRand | F::ArrayFirst | F::ArrayLast | F::ArrayIsList
            | F::Natsort | F::Natcasesort | F::ArrayMerge | F::ArrayPad | F::Shuffle
    )
}

/// How a runtime-promoted hash operand is made readable by a builtin's packed lowering.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PromotedArm {
    /// Re-run the builtin's associative lowering with the operand typed `array<int, T>`.
    HashLowering,
    /// Replace the receiver (operand 0) in place by the packed list of its values.
    ReceiverToList,
    /// Substitute a temporary packed list of the values for the operand, then restore it.
    TempList,
}

/// Picks how `target` reads a promoted hash operand.
fn promoted_arm(target: RuntimeFnId, operand_index: usize) -> PromotedArm {
    match target {
        RuntimeFnId::ArraySplice | RuntimeFnId::Usort | RuntimeFnId::Shuffle
            if operand_index == 0 =>
        {
            PromotedArm::ReceiverToList
        }
        RuntimeFnId::ArrayMerge | RuntimeFnId::ArrayPad => PromotedArm::TempList,
        _ => PromotedArm::HashLowering,
    }
}

/// Lowers one typed runtime function, routing runtime-promoted arrays to the hash lowering.
///
/// A sparse integer write (`$a = [1, 2]; $a[7] = 3;`) promotes an `array<T>` to hash storage at
/// run time while its static type stays `array<T>`. For the builtins that read storage through the
/// static type, the heap kind of each of the first two statically-packed array operands is tested
/// (one call plus a compare on the packed path); a hash is handled as `promoted_arm` says.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    if consumes_possibly_promoted_array(target) {
        // An `array<mixed>` is how a declared `array` is typed, and it may legitimately hold
        // string keys; the key type this dispatch substitutes (`int`) would be a lie for it, and
        // those consumers already read the storage kind themselves.
        let packed: Vec<(usize, ValueId)> = inst
            .operands
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, operand)| {
                matches!(
                    ctx.value_php_type(*operand),
                    Ok(PhpType::Array(element))
                        if !matches!(
                            element.codegen_repr(),
                            PhpType::Mixed | PhpType::Void | PhpType::Never
                        )
                ) && !ctx.value_type_overrides.contains_key(operand)
            })
            .take(if promoted_arm(target, 0) == PromotedArm::ReceiverToList { 1 } else { 2 })
            .collect();
        if !packed.is_empty() {
            // The hash arm reuses the builtin's associative lowering, which does not exist for
            // every element type. Rather than turning a program that compiled (and worked while
            // the list stayed packed) into a compile error, a dispatch that cannot be lowered is
            // dropped and the packed lowering stands alone, as it did before this dispatch.
            let checkpoint = ctx.emitter.checkpoint();
            let dispatched = lower_promoted_operands(ctx, inst, target, &packed);
            if dispatched.is_ok() {
                return dispatched;
            }
            for (_, operand) in &packed {
                ctx.value_type_overrides.remove(operand);
            }
            ctx.emitter.rollback_to(checkpoint);
        }
    }
    lower_by_static_type(ctx, inst, target)
}

/// Emits the storage-kind dispatch for `operands[0]`, nesting one for each further operand.
fn lower_promoted_operands(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
    operands: &[(usize, ValueId)],
) -> Result<()> {
    let Some((&(index, array), rest)) = operands.split_first() else {
        return lower_by_static_type(ctx, inst, target);
    };
    let PhpType::Array(element) = ctx.value_php_type(array)? else {
        return lower_promoted_operands(ctx, inst, target, rest);
    };
    super::builtins::arrays::lower_by_runtime_storage_kind(
        ctx,
        array,
        |ctx| lower_promoted_operands(ctx, inst, target, rest),
        |ctx| match promoted_arm(target, index) {
            PromotedArm::ReceiverToList => {
                super::builtins::arrays::emit_hash_receiver_to_packed_list(
                    ctx,
                    array,
                    &element,
                    target.as_eir(),
                )?;
                lower_promoted_operands(ctx, inst, target, rest)
            }
            PromotedArm::TempList => {
                super::builtins::arrays::emit_promoted_operand_as_temp_list(ctx, array, &element)?;
                let result = lower_promoted_operands(ctx, inst, target, rest);
                super::builtins::arrays::emit_restore_promoted_operand(ctx, array)?;
                result
            }
            PromotedArm::HashLowering => {
                ctx.value_type_overrides.insert(
                    array,
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Int),
                        value: element.clone(),
                    },
                );
                let result = lower_promoted_operands(ctx, inst, target, rest);
                ctx.value_type_overrides.remove(&array);
                result
            }
        },
    )
}


/// Lowers one typed runtime function through its backend implementation dispatcher.
fn lower_by_static_type(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    if let Some(result) = group_00::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_01::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_02::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_03::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_04::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_05::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_06::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_07::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_08::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_09::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_10::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_11::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_12::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_13::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_14::lower(ctx, inst, target) {
        return result;
    }
    Err(CodegenIrError::invalid_module(format!(
        "missing backend dispatch for typed builtin target {}",
        target.as_eir(),
    )))
}
