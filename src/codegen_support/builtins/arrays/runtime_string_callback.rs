//! Purpose:
//! Emits descriptor-backed runtime string callback dispatch for array callback builtins.
//! Shared by callbacks that evaluate and save their source array before callback selection.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::array_filter::emit()`.
//! - `crate::codegen_support::builtins::arrays::array_reduce::emit()`.
//! - `crate::codegen_support::builtins::arrays::array_walk::emit()`.
//!
//! Key details:
//! - The caller must have pushed the source array before this helper evaluates the callback.
//! - The saved array remains below the runtime string slot while descriptor cases are checked.

use crate::codegen_support::abi;
use crate::codegen_support::callable_dispatch::{self, RuntimeCallableSelector};
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::call_user_func_array;
use super::callback_env::{self, DescriptorCallbackEnv};

const SAVED_ARRAY_AFTER_STRING_OFFSET: usize = 16;

/// Emits runtime string callback dispatch after the caller has saved the source array.
///
/// Returns `true` after consuming the saved array and runtime string stack slots. Returns
/// `false` without emitting code when the callback is not a runtime string expression.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_after_saved_array<F>(
    callback: &Expr,
    source_arg_ty: Option<&PhpType>,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    array_arg_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    mut emit_call: F,
) -> bool
where
    F: FnMut(&DescriptorCallbackEnv, &mut Emitter, &mut Context, &mut DataSection),
{
    if !call_user_func_array::callback_is_runtime_string(callback, ctx) {
        return false;
    }

    let call_reg = abi::nested_call_reg(emitter);
    let callback_ty = emit_expr(callback, emitter, ctx, data);
    debug_assert!(matches!(callback_ty.codegen_repr(), PhpType::Str));
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                        // preserve the runtime string callback name above the saved source array

    let cases = callable_dispatch::runtime_callable_cases(ctx, data, &[], source_arg_ty);
    let done_label = ctx.next_label("array_runtime_string_callback_done");
    let selector = RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg,
    };

    for case in &cases {
        let next_case = ctx.next_label("array_runtime_string_callback_next");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            emitter,
            ctx,
            data,
        );
        abi::emit_load_temporary_stack_slot(
            emitter,
            array_arg_reg,
            SAVED_ARRAY_AFTER_STRING_OFFSET,
        );
        let wrapper = callback_env::emit_descriptor_callback_env_from_static_descriptor(
            &case.descriptor_label,
            visible_arg_types.clone(),
            Vec::new(),
            descriptor_return_type.clone(),
            emitter,
            ctx,
        );
        callback_env::store_descriptor_callback_array_reg(&wrapper, array_arg_reg, emitter);
        emit_call(&wrapper, emitter, ctx, data);
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }

    call_user_func_array::emit_dynamic_string_callback_abort(emitter, data);
    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 32);                             // discard the runtime string callback name and saved source array
    true
}
