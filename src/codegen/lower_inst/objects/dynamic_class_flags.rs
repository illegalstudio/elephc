//! Purpose:
//! Lowers the dynamic abstract-class predicate from shared AOT class metadata.
//!
//! Called from:
//! - The typed EIR instruction dispatcher for tokenizer construction preconditions.
//!
//! Key details:
//! - Shared ABI helpers implement the same lookup on every supported target.
//! - This predicate never allocates a token or invokes a user destructor.

use super::*;

/// Lowers the internal class-name predicate used before tokenizer late-static construction.
pub(in crate::codegen::lower_inst) fn lower_dynamic_class_is_abstract(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let false_label = ctx.next_label("dynamic_is_abstract_false");
    let done_label = ctx.next_label("dynamic_is_abstract_done");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &false_label)? {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return store_if_result(ctx, inst);
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let mut classes = ctx
        .module
        .class_infos
        .iter()
        .filter(|(_, info)| info.is_abstract)
        .collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    let matched_labels = classes
        .iter()
        .map(|(class_name, _)| {
            let label = ctx.next_label("dynamic_is_abstract_match");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
            label
        })
        .collect::<Vec<_>>();

    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);
    for label in matched_labels {
        ctx.emitter.label(&label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}
