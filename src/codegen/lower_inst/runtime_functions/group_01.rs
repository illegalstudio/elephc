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

/// Lowers a target owned by bounded dispatch group 01, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::ArrayShift => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_shift(ctx, inst)
        }),
        RuntimeFnId::ArraySlice => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_slice(ctx, inst)
        }),
        RuntimeFnId::ArraySplice => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_splice(ctx, inst)
        }),
        RuntimeFnId::ArraySum => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_sum(ctx, inst)
        }),
        RuntimeFnId::ArrayUdiff => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_udiff(ctx, inst)
        }),
        RuntimeFnId::ArrayUintersect => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_uintersect(ctx, inst)
        }),
        RuntimeFnId::ArrayUnique => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_unique(ctx, inst)
        }),
        RuntimeFnId::ArrayUnshift => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_unshift(ctx, inst)
        }),
        RuntimeFnId::ArrayValues => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_values(ctx, inst)
        }),
        RuntimeFnId::ArrayWalk => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_walk(ctx, inst)
        }),
        RuntimeFnId::ArrayWalkRecursive => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_walk_recursive(ctx, inst)
        }),
        RuntimeFnId::Arsort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_arsort(ctx, inst)
        }),
        RuntimeFnId::Asort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_asort(ctx, inst)
        }),
        RuntimeFnId::Count => Some({
            crate::codegen::lower_inst::builtins::lower_count(ctx, inst)
        }),
        RuntimeFnId::InArray => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_in_array(ctx, inst)
        }),
        RuntimeFnId::Krsort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_krsort(ctx, inst)
        }),
        RuntimeFnId::Ksort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_ksort(ctx, inst)
        }),
        RuntimeFnId::Natcasesort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_natcasesort(ctx, inst)
        }),
        RuntimeFnId::Natsort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_natsort(ctx, inst)
        }),
        RuntimeFnId::Range => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_range(ctx, inst)
        }),
        RuntimeFnId::Rsort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_rsort(ctx, inst)
        }),
        RuntimeFnId::Shuffle => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_shuffle(ctx, inst)
        }),
        RuntimeFnId::Sort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_sort(ctx, inst)
        }),
        RuntimeFnId::Uasort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_uasort(ctx, inst)
        }),
        RuntimeFnId::Uksort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_uksort(ctx, inst)
        }),
        RuntimeFnId::Usort => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_usort(ctx, inst)
        }),
        RuntimeFnId::CallUserFunc => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_call_user_func_builtin_escape(ctx, inst, "call_user_func")
        }),
        RuntimeFnId::CallUserFuncArray => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_call_user_func_builtin_escape(ctx, inst, "call_user_func_array")
        }),
        RuntimeFnId::ClassAlias => Some({
            crate::codegen::lower_inst::builtins::types::lower_class_alias(ctx, inst)
        }),
        RuntimeFnId::ClassExists => Some({
            crate::codegen::lower_inst::builtins::lower_class_like_exists(ctx, inst, "class_exists")
        }),
        RuntimeFnId::ClassImplements => Some({
            crate::codegen::lower_inst::builtins::class_relations::lower_class_relation(
                    ctx,
                    inst,
                    "class_implements",
                )
        }),
        RuntimeFnId::ClassParents => Some({
            crate::codegen::lower_inst::builtins::class_relations::lower_class_relation(
                    ctx,
                    inst,
                    "class_parents",
                )
        }),
        RuntimeFnId::ClassUses => Some({
            crate::codegen::lower_inst::builtins::class_relations::lower_class_relation(
                    ctx,
                    inst,
                    "class_uses",
                )
        }),
        RuntimeFnId::EnumExists => Some({
            crate::codegen::lower_inst::builtins::lower_class_like_exists(ctx, inst, "enum_exists")
        }),
        RuntimeFnId::FunctionExists => Some({
            crate::codegen::lower_inst::builtins::lower_function_exists(ctx, inst)
        }),
        _ => None,
    }
}
