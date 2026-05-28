//! Purpose:
//! Builds raw descriptor-invoker argument arrays without generic array-literal spread lowering.
//! Handles positional prefixes followed by indexed spread sources for callable invoker paths.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::callable_forms`
//! - `crate::codegen::expr::calls::descriptor_invoker_args`
//!
//! Key details:
//! - The destination array uses boxed Mixed slots so descriptor invokers can apply metadata at runtime.
//! - Spread sources are cloned to Mixed slots before merging, preserving string lengths and refcounted payloads.

use crate::codegen::abi;
use crate::codegen::builtins::arrays::call_user_func_array;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::expr::calls::args as call_args;
use crate::codegen::functions;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

/// Emits an indexed Mixed argument array, optionally storing variable args as ref-cell markers.
pub(crate) fn emit_indexed_invoker_arg_array(
    args: &[Expr],
    encode_variable_refs: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("descriptor invoker indexed argument array");
    emit_new_mixed_indexed_array(args.len().max(4), emitter);
    emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor argument array alive while filling Mixed slots

    for (i, arg) in args.iter().enumerate() {
        if encode_variable_refs {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !call_args::emit_ref_arg_variable_address(
                    var_name,
                    "descriptor invoker arg",
                    emitter,
                    ctx,
                ) {
                    panic!("descriptor invoker argument variable not found");
                }
                emit_box_current_ref_arg_address_for_invoker(var_name, emitter, ctx);
                emit_store_current_mixed_slot(i, emitter);
                continue;
            }
        }

        emit_store_next_mixed_slot(arg, i, emitter, ctx, data);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the filled descriptor argument array
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Emits an indexed Mixed argument array with a saved object receiver in slot zero.
pub(crate) fn emit_indexed_invoker_arg_array_with_saved_object_prefix(
    object_stack_offset: usize,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    encode_variable_refs: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("descriptor invoker receiver-prefixed indexed argument array");
    emit_new_mixed_indexed_array((args.len() + 1).max(4), emitter);
    emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the receiver-prefixed descriptor argument array alive while filling Mixed slots
    emit_store_saved_object_prefix_slot(object_stack_offset + 16, 0, emitter);

    for (idx, arg) in args.iter().enumerate() {
        emit_store_invoker_arg_slot(
            arg,
            idx + 1,
            sig,
            encode_variable_refs,
            "descriptor invoker receiver-prefixed arg",
            emitter,
            ctx,
            data,
        );
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the filled receiver-prefixed descriptor argument array
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Emits a raw indexed argument array for positional args plus indexed spreads.
pub(crate) fn emit_positional_spread_invoker_arg_array(
    leading_args: &[Expr],
    args: &[Expr],
    sig: Option<&FunctionSig>,
    encode_variable_refs: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let plan = positional_spread_plan(args, ctx)?;
    emitter.comment("descriptor invoker positional spread argument array");
    emit_new_mixed_indexed_array((leading_args.len() + plan.prefix_args.len()).max(16), emitter);
    emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor argument array alive while positional slots and spreads are appended

    let mut slot = 0usize;
    for arg in leading_args {
        emit_store_invoker_arg_slot(
            arg,
            slot,
            sig,
            encode_variable_refs,
            "descriptor invoker leading arg",
            emitter,
            ctx,
            data,
        );
        slot += 1;
    }
    for arg in plan.prefix_args {
        emit_store_invoker_arg_slot(
            arg,
            slot,
            sig,
            encode_variable_refs,
            "descriptor invoker spread-prefix arg",
            emitter,
            ctx,
            data,
        );
        slot += 1;
    }
    for (spread, elem_ty) in plan.spreads {
        emit_merge_indexed_spread(spread, &elem_ty, emitter, ctx, data);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed positional-spread argument array
    Some(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Emits a raw indexed argument array with a saved object receiver followed by positional args/spreads.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_positional_spread_invoker_arg_array_with_saved_object_prefix(
    object_stack_offset: usize,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    encode_variable_refs: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let plan = positional_spread_plan(args, ctx)?;
    emitter.comment("descriptor invoker receiver-prefixed positional spread argument array");
    emit_new_mixed_indexed_array((plan.prefix_args.len() + 1).max(16), emitter);
    emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the receiver-prefixed descriptor argument array alive while positional slots and spreads are appended

    let mut slot = 0usize;
    emit_store_saved_object_prefix_slot(object_stack_offset + 16, slot, emitter);
    slot += 1;
    for arg in plan.prefix_args {
        emit_store_invoker_arg_slot(
            arg,
            slot,
            sig,
            encode_variable_refs,
            "descriptor invoker receiver-prefixed spread-prefix arg",
            emitter,
            ctx,
            data,
        );
        slot += 1;
    }
    for (spread, elem_ty) in plan.spreads {
        emit_merge_indexed_spread(spread, &elem_ty, emitter, ctx, data);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed receiver-prefixed positional-spread argument array
    Some(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Stores one positional descriptor argument, preserving variable storage when runtime by-ref metadata may need it.
#[allow(clippy::too_many_arguments)]
fn emit_store_invoker_arg_slot(
    arg: &Expr,
    index: usize,
    sig: Option<&FunctionSig>,
    encode_variable_refs: bool,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if should_encode_variable_ref_arg(sig, index, arg, encode_variable_refs) {
        if let ExprKind::Variable(var_name) = &arg.kind {
            if !call_args::emit_ref_arg_variable_address(var_name, context_label, emitter, ctx) {
                panic!("descriptor invoker argument variable not found");
            }
            emit_box_current_ref_arg_address_for_invoker(var_name, emitter, ctx);
            emit_store_current_mixed_slot(index, emitter);
            return;
        }
    }

    emit_store_next_mixed_slot(arg, index, emitter, ctx, data);
}

/// Returns true when this positional descriptor slot should carry an invoker ref-cell marker.
fn should_encode_variable_ref_arg(
    sig: Option<&FunctionSig>,
    index: usize,
    arg: &Expr,
    encode_variable_refs: bool,
) -> bool {
    encode_variable_refs
        && matches!(arg.kind, ExprKind::Variable(_))
        && sig.is_none_or(|sig| sig.ref_params.get(index).copied().unwrap_or(false))
}

/// Plans a positional-only argument list with one or more indexed spread tails.
fn positional_spread_plan<'a>(args: &'a [Expr], ctx: &Context) -> Option<PositionalSpreadPlan<'a>> {
    let mut prefix_args = Vec::new();
    let mut spreads = Vec::new();
    let mut seen_spread = false;

    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => {
                seen_spread = true;
                let elem_ty = indexed_spread_element_type(inner, ctx)?;
                spreads.push((inner.as_ref(), elem_ty));
            }
            ExprKind::NamedArg { .. } => return None,
            _ if seen_spread => return None,
            _ => prefix_args.push(arg),
        }
    }

    if spreads.is_empty() {
        return None;
    }

    Some(PositionalSpreadPlan {
        prefix_args,
        spreads,
    })
}

/// Boxes a saved object pointer and stores it into a descriptor argument slot.
fn emit_store_saved_object_prefix_slot(
    object_stack_offset: usize,
    index: usize,
    emitter: &mut Emitter,
) {
    let object_reg = abi::secondary_scratch_reg(emitter);
    let zero_reg = abi::tertiary_scratch_reg(emitter);
    let tag_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, object_reg, object_stack_offset);
    abi::emit_load_int_immediate(emitter, zero_reg, 0);
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(&PhpType::Object(String::new())) as i64,
    );
    crate::codegen::emit_box_runtime_payload_as_mixed(emitter, tag_reg, object_reg, zero_reg);
    emit_store_current_mixed_slot(index, emitter);
}

/// Returns the element type for a spread source when it is statically indexed-array-shaped.
fn indexed_spread_element_type(spread: &Expr, ctx: &Context) -> Option<PhpType> {
    match functions::infer_contextual_type(spread, ctx).codegen_repr() {
        PhpType::Array(elem_ty) => Some(*elem_ty),
        _ => None,
    }
}

/// Allocates an indexed array with Mixed slots for descriptor invoker arguments.
fn emit_new_mixed_indexed_array(capacity: usize, emitter: &mut Emitter) {
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let elem_size_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, capacity_reg, capacity as i64);
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
}

/// Boxes `arg` as Mixed and stores it into the destination slot at `index`.
fn emit_store_next_mixed_slot(
    arg: &Expr,
    index: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let mut ty = emit_expr(arg, emitter, ctx, data);
    let boxed_iterable = crate::codegen::emit_box_iterable_value_for_mixed_container(
        emitter,
        &mut ty,
    );
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_expr_value_as_mixed_for_container(emitter, arg, &ty);
    } else if !boxed_iterable {
        retain_borrowed_mixed_arg(emitter, arg, &ty);
    }
    emit_store_current_mixed_slot(index, emitter);
}

/// Retains a borrowed Mixed payload before storing it in the invoker container.
fn retain_borrowed_mixed_arg(emitter: &mut Emitter, arg: &Expr, ty: &PhpType) {
    if ty.codegen_repr().is_refcounted() && expr_result_heap_ownership(arg) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, &ty.codegen_repr());
    }
}

/// Stores the current boxed Mixed value into the destination argument array.
fn emit_store_current_mixed_slot(index: usize, emitter: &mut Emitter) {
    let array_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, 0);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), array_reg, 24 + index * 8);
    abi::emit_load_int_immediate(emitter, len_reg, (index + 1) as i64);
    abi::emit_store_to_address(emitter, len_reg, array_reg, 0);
}

/// Boxes the current variable storage address as an invoker-only Mixed marker.
pub(crate) fn emit_box_current_ref_arg_address_for_invoker(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &Context,
) {
    let ref_cell_reg = abi::secondary_scratch_reg(emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(emitter);
    let source_tag_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", ref_cell_reg, abi::int_result_reg(emitter))); // preserve the source variable storage address before Mixed marker boxing
    abi::emit_load_int_immediate(
        emitter,
        marker_tag_reg,
        call_user_func_array::INVOKER_ARG_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        emitter,
        source_tag_reg,
        variable_runtime_value_tag(var_name, ctx) as i64,
    );
    crate::codegen::emit_box_runtime_payload_as_mixed(
        emitter,
        marker_tag_reg,
        ref_cell_reg,
        source_tag_reg,
    );
}

/// Returns the runtime tag for a variable's current codegen type.
fn variable_runtime_value_tag(var_name: &str, ctx: &Context) -> u8 {
    ctx.variables
        .get(var_name)
        .map(|var| crate::codegen::runtime_value_tag(&var.ty.codegen_repr()))
        .unwrap_or_else(|| crate::codegen::runtime_value_tag(&PhpType::Int))
}

/// Appends an indexed spread source to the destination Mixed argument array.
fn emit_merge_indexed_spread(
    spread: &Expr,
    inferred_elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let spread_ty = emit_expr(spread, emitter, ctx, data);
    let elem_ty = match spread_ty {
        PhpType::Array(elem_ty) => *elem_ty,
        _ => inferred_elem_ty.clone(),
    };
    call_user_func_array::emit_clone_indexed_array_for_invoker(
        abi::int_result_reg(emitter),
        &elem_ty,
        emitter,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the cloned Mixed spread source while merging it into the destination array

    let dest_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let source_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_temporary_stack_slot(emitter, dest_arg_reg, 16);
    abi::emit_load_temporary_stack_slot(emitter, source_arg_reg, 0);
    abi::emit_call_label(emitter, "__rt_array_merge_into_refcounted");
    abi::emit_store_to_address(
        emitter,
        abi::int_result_reg(emitter),
        temporary_stack_reg(emitter),
        16,
    );

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the merged destination while releasing the cloned spread source
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the merged descriptor argument array after source-clone release
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the cloned spread-source stack slot
}

/// Returns the active stack pointer register for direct temporary-slot stores.
fn temporary_stack_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "sp",
        crate::codegen::platform::Arch::X86_64 => "rsp",
    }
}

/// Borrowed view of a positional-spread descriptor argument list.
struct PositionalSpreadPlan<'a> {
    prefix_args: Vec<&'a Expr>,
    spreads: Vec<(&'a Expr, PhpType)>,
}
