//! Purpose:
//! Loads the current value of a hash foreach as an owned object without a temporary Mixed box.
//!
//! Called from:
//! - `super::lower_iter_current_value()` for hash sources whose EIR result is a concrete object.
//!
//! Key details:
//! - An ordinary entry tagged as a live object hands out its payload with one new reference.
//! - Every other entry (PHP references, nested boxes, null containers) keeps the boxed path and
//!   then unboxes it, so the bound value matches the former `IterCurrentValue` + `MixedUnbox` pair.

use super::*;
use crate::codegen::lower_inst::objects::OBJECT_TAG;

/// Emits the owned object current value of a hash iterator into the integer result register.
pub(super) fn load_current_hash_object_value(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    preserve_reference: bool,
    promote_reference: bool,
) {
    let object_ty = PhpType::Object(String::new());
    let boxed = ctx.next_label("iter_hash_object_boxed");
    let done = ctx.next_label("iter_hash_object_done");
    if !preserve_reference && !promote_reference {
        let result = abi::int_result_reg(ctx.emitter);
        let (tag, scratch) = match ctx.emitter.target.arch {
            Arch::AArch64 => ("x9", "x10"),
            Arch::X86_64 => ("r10", "r11"),
        };
        abi::load_at_offset(ctx.emitter, tag, offset - ITER_VALUE_TAG_OFFSET_DELTA);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {tag}, #{OBJECT_TAG}"));  // is the saved hash entry a plain object payload?
                ctx.emitter.instruction(&format!("b.ne {boxed}"));              // references and other tags keep the boxed path
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {tag}, {OBJECT_TAG}"));   // is the saved hash entry a plain object payload?
                ctx.emitter.instruction(&format!("jne {boxed}"));               // references and other tags keep the boxed path
            }
        }
        abi::load_at_offset(ctx.emitter, result, offset - ITER_VALUE_LO_OFFSET_DELTA);
        crate::codegen::sentinels::emit_branch_if_null_container(ctx.emitter, result, scratch, &boxed);
        abi::emit_incref_if_refcounted(ctx.emitter, &object_ty);
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&boxed);
    load_current_hash_call_value_as_mixed(ctx, offset, preserve_reference, promote_reference);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    super::super::reference_arguments::emit_unbox_mixed_to_owned_refcounted_result(ctx, &object_ty);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.label(&done);
}
