//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 00, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::ArrayAll => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_all(ctx, inst)
        }),
        RuntimeFnId::ArrayAny => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_any(ctx, inst)
        }),
        RuntimeFnId::ArrayChunk => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_chunk(ctx, inst)
        }),
        RuntimeFnId::ArrayColumn => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_column(ctx, inst)
        }),
        RuntimeFnId::ArrayCombine => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_combine(ctx, inst)
        }),
        RuntimeFnId::ArrayDiff => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_diff(ctx, inst)
        }),
        RuntimeFnId::ArrayDiffAssoc => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_diff_assoc(ctx, inst)
        }),
        RuntimeFnId::ArrayDiffKey => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_diff_key(ctx, inst)
        }),
        RuntimeFnId::ArrayFill => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_fill(ctx, inst)
        }),
        RuntimeFnId::ArrayFillKeys => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_fill_keys(ctx, inst)
        }),
        RuntimeFnId::ArrayFilter => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_filter(ctx, inst)
        }),
        RuntimeFnId::ArrayFind => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_find(ctx, inst)
        }),
        RuntimeFnId::ArrayFlip => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_flip(ctx, inst)
        }),
        RuntimeFnId::ArrayIntersect => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_intersect(ctx, inst)
        }),
        RuntimeFnId::ArrayIntersectAssoc => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_intersect_assoc(ctx, inst)
        }),
        RuntimeFnId::ArrayIntersectKey => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_intersect_key(ctx, inst)
        }),
        RuntimeFnId::ArrayIsList => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_is_list(ctx, inst)
        }),
        RuntimeFnId::ArrayKeyExists => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_key_exists(ctx, inst)
        }),
        RuntimeFnId::ArrayKeyFirst => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_key_first(ctx, inst)
        }),
        RuntimeFnId::ArrayKeyLast => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_key_last(ctx, inst)
        }),
        RuntimeFnId::ArrayKeys => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_keys(ctx, inst)
        }),
        RuntimeFnId::ArrayMap => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_map(ctx, inst)
        }),
        RuntimeFnId::ArrayMerge => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_merge(ctx, inst)
        }),
        RuntimeFnId::ArrayMergeRecursive => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_merge_recursive(ctx, inst)
        }),
        RuntimeFnId::ArrayMultisort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_multisort(ctx, inst)
        }),
        RuntimeFnId::ArrayPad => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_pad(ctx, inst)
        }),
        RuntimeFnId::ArrayPop => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_pop(ctx, inst)
        }),
        RuntimeFnId::ArrayProduct => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_product(ctx, inst)
        }),
        RuntimeFnId::ArrayPush => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_push(ctx, inst)
        }),
        RuntimeFnId::ArrayRand => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_rand(ctx, inst)
        }),
        RuntimeFnId::ArrayReduce => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_reduce(ctx, inst)
        }),
        RuntimeFnId::ArrayReplace => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_replace(ctx, inst)
        }),
        RuntimeFnId::ArrayReplaceRecursive => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_replace_recursive(ctx, inst)
        }),
        RuntimeFnId::ArrayReverse => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_reverse(ctx, inst)
        }),
        RuntimeFnId::ArraySearch => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_search(ctx, inst)
        }),
        _ => None,
    }
}
