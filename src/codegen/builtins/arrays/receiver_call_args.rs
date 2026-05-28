//! Purpose:
//! Builds receiver-prefixed argument containers for descriptor-based callable invocation.
//! Converts receiver-bound call_user_func_array() inputs into boxed Mixed containers.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::callable_forms`
//! - `crate::codegen::expr::calls::callable_array_runtime`
//!
//! Key details:
//! - The synthetic receiver occupies descriptor argument slot zero; numeric source keys shift by one.
//! - Source argument arrays are cloned/retained before rewriting so caller-visible containers stay unchanged.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::call_user_func_array;

const HASH_SCRATCH_BYTES: usize = 48;
const CURSOR_OFF: usize = 0;
const KEY_PTR_OFF: usize = 8;
const KEY_LEN_OFF: usize = 16;
const VALUE_LO_OFF: usize = 24;
const VALUE_HI_OFF: usize = 32;
const VALUE_TAG_OFF: usize = 40;

/// Emits a boxed Mixed argument container for dynamic receiver-bound call_user_func_array() args.
///
/// Returns `true` when `arg_array_ty` is a supported dynamic container shape and
/// leaves the boxed Mixed container in the integer result register. Indexed arrays
/// become `[receiver, ...$args]`; associative hashes become a new hash whose
/// numeric keys are shifted by one and whose string keys are preserved.
pub(crate) fn emit_receiver_prefixed_dynamic_arg_mixed(
    receiver: &Expr,
    arg_array: &Expr,
    arg_array_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    match arg_array_ty {
        PhpType::Array(arg_elem_ty) => {
            emit_receiver_prefixed_indexed_arg_mixed(
                receiver,
                arg_array,
                arg_elem_ty.as_ref(),
                emitter,
                ctx,
                data,
            );
            true
        }
        PhpType::AssocArray { .. } => {
            emit_receiver_prefixed_assoc_arg_mixed(receiver, arg_array, emitter, ctx, data);
            true
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_receiver_prefixed_opaque_arg_mixed(receiver, arg_array, emitter, ctx, data);
            true
        }
        _ => false,
    }
}

/// Emits a boxed Mixed argument container using a receiver pointer already saved on the temp stack.
pub(crate) fn emit_saved_receiver_prefixed_dynamic_arg_mixed(
    object_stack_offset: usize,
    arg_array: &Expr,
    arg_array_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    match arg_array_ty.codegen_repr() {
        PhpType::Array(arg_elem_ty) => {
            emit_saved_receiver_prefixed_indexed_arg_mixed(
                object_stack_offset,
                arg_array,
                arg_elem_ty.as_ref(),
                emitter,
                ctx,
                data,
            );
            true
        }
        PhpType::AssocArray { .. } => {
            emit_saved_receiver_prefixed_assoc_arg_mixed(
                object_stack_offset,
                arg_array,
                emitter,
                ctx,
                data,
            );
            true
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_saved_receiver_prefixed_opaque_arg_mixed(
                object_stack_offset,
                arg_array,
                emitter,
                ctx,
                data,
            );
            true
        }
        _ => false,
    }
}

/// Builds a boxed Mixed argument container `[receiver, ...$args]` for indexed arrays.
fn emit_receiver_prefixed_indexed_arg_mixed(
    receiver: &Expr,
    arg_array: &Expr,
    inferred_arg_elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("receiver-prefixed indexed call_user_func_array descriptor args");
    let receiver_ty = emit_expr(receiver, emitter, ctx, data);
    crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
        emitter,
        receiver,
        &receiver_ty,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed receiver Mixed cell before evaluating the source indexed array

    let arr_ty = emit_expr(arg_array, emitter, ctx, data);
    let source_elem_ty = match &arr_ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref(),
        _ => inferred_arg_elem_ty,
    };
    emit_receiver_prefixed_indexed_payload_arg_mixed(source_elem_ty, emitter);
}

/// Builds `[saved receiver, ...$args]` for indexed arrays.
fn emit_saved_receiver_prefixed_indexed_arg_mixed(
    object_stack_offset: usize,
    arg_array: &Expr,
    inferred_arg_elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("saved receiver-prefixed indexed call_user_func_array descriptor args");
    emit_push_saved_object_receiver_mixed(object_stack_offset, emitter);

    let arr_ty = emit_expr(arg_array, emitter, ctx, data);
    let source_elem_ty = match &arr_ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref(),
        _ => inferred_arg_elem_ty,
    };
    emit_receiver_prefixed_indexed_payload_arg_mixed(source_elem_ty, emitter);
}

/// Builds `[receiver, ...$args]` from a loaded raw indexed-array payload.
fn emit_receiver_prefixed_indexed_payload_arg_mixed(
    source_elem_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let result_reg = abi::int_result_reg(emitter);
    call_user_func_array::emit_clone_indexed_array_for_invoker(
        result_reg,
        source_elem_ty,
        emitter,
    );
    emit_receiver_prefixed_indexed_clone_arg_mixed(emitter);
}

/// Builds `[receiver, ...$args]` from a loaded runtime-typed indexed-array payload.
fn emit_receiver_prefixed_runtime_indexed_payload_arg_mixed(emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    call_user_func_array::emit_clone_indexed_array_for_invoker_with_runtime_tag(
        result_reg,
        emitter,
    );
    emit_receiver_prefixed_indexed_clone_arg_mixed(emitter);
}

/// Builds the receiver-prefixed destination from a cloned Mixed indexed-array payload.
fn emit_receiver_prefixed_indexed_clone_arg_mixed(emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the cloned Mixed source array before allocating the receiver-prefixed container

    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let elem_size_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, capacity_reg, 16);
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    emit_array_value_type_stamp(emitter, result_reg, &PhpType::Mixed);

    let scratch_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, scratch_reg, 16);
    abi::emit_store_to_address(emitter, scratch_reg, result_reg, 24);
    abi::emit_load_int_immediate(emitter, scratch_reg, 1);
    abi::emit_store_to_address(emitter, scratch_reg, result_reg, 0);

    abi::emit_push_reg(emitter, result_reg);                                    // preserve the destination array while merging the cloned source tail
    let dest_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let source_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_temporary_stack_slot(emitter, dest_arg_reg, 0);
    abi::emit_load_temporary_stack_slot(emitter, source_arg_reg, 16);
    abi::emit_call_label(emitter, "__rt_array_merge_into_refcounted");

    let normalized_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the merged receiver-prefixed array while releasing the source clone
    abi::emit_load_temporary_stack_slot(emitter, result_reg, 32);
    abi::emit_decref_if_refcounted(emitter, &normalized_array_ty);
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the merged receiver-prefixed array after source-clone release
    abi::emit_release_temporary_stack(emitter, 48);                             // discard stale destination, source-clone, and receiver stack slots
    emit_box_receiver_prefixed_container(result_reg, &normalized_array_ty, emitter);
}

/// Builds a boxed Mixed hash with the receiver in numeric slot zero.
fn emit_receiver_prefixed_assoc_arg_mixed(
    receiver: &Expr,
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("receiver-prefixed assoc call_user_func_array descriptor args");
    let receiver_ty = emit_expr(receiver, emitter, ctx, data);
    crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
        emitter,
        receiver,
        &receiver_ty,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed receiver Mixed cell before evaluating the source hash

    let _ = emit_expr(arg_array, emitter, ctx, data);
    emit_receiver_prefixed_assoc_payload_arg_mixed(emitter, ctx);
}

/// Builds a boxed Mixed hash with a saved receiver in numeric slot zero.
fn emit_saved_receiver_prefixed_assoc_arg_mixed(
    object_stack_offset: usize,
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("saved receiver-prefixed assoc call_user_func_array descriptor args");
    emit_push_saved_object_receiver_mixed(object_stack_offset, emitter);

    let _ = emit_expr(arg_array, emitter, ctx, data);
    emit_receiver_prefixed_assoc_payload_arg_mixed(emitter, ctx);
}

/// Builds a receiver-prefixed hash from a loaded raw associative-array payload.
fn emit_receiver_prefixed_assoc_payload_arg_mixed(emitter: &mut Emitter, ctx: &mut Context) {
    let result_reg = abi::int_result_reg(emitter);
    call_user_func_array::emit_clone_assoc_array_for_invoker(result_reg, emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the cloned Mixed source hash before allocating the receiver-prefixed hash

    emit_new_receiver_prefixed_hash(0, emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the destination hash while inserting receiver and shifted source entries
    emit_insert_receiver_hash_entry(emitter);
    emit_copy_shifted_assoc_hash_entries(emitter, ctx);

    let normalized_hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    abi::emit_load_temporary_stack_slot(emitter, result_reg, 16);
    abi::emit_decref_if_refcounted(emitter, &normalized_hash_ty);
    abi::emit_load_temporary_stack_slot(emitter, result_reg, 0);
    abi::emit_release_temporary_stack(emitter, 48);                             // discard destination, source-clone, and receiver stack slots
    emit_box_receiver_prefixed_container(result_reg, &normalized_hash_ty, emitter);
}

/// Builds a receiver-prefixed argument container from a runtime Mixed array/hash.
fn emit_receiver_prefixed_opaque_arg_mixed(
    receiver: &Expr,
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("receiver-prefixed mixed call_user_func_array descriptor args");
    let receiver_ty = emit_expr(receiver, emitter, ctx, data);
    crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
        emitter,
        receiver,
        &receiver_ty,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed receiver Mixed cell before evaluating the opaque source container

    emit_receiver_prefixed_opaque_arg_mixed_after_receiver_push(arg_array, emitter, ctx, data);
}

/// Builds a receiver-prefixed argument container after the boxed receiver was pushed.
fn emit_receiver_prefixed_opaque_arg_mixed_after_receiver_push(
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let _ = emit_expr(arg_array, emitter, ctx, data);
    let mixed_reg = abi::int_result_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let payload_reg = abi::tertiary_scratch_reg(emitter);
    let indexed_label = ctx.next_label("receiver_mixed_indexed_args");
    let assoc_label = ctx.next_label("receiver_mixed_assoc_args");
    let done_label = ctx.next_label("receiver_mixed_args_done");
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };

    abi::emit_load_from_address(emitter, tag_reg, mixed_reg, 0);
    abi::emit_load_from_address(emitter, payload_reg, mixed_reg, 8);
    abi::emit_push_reg(emitter, payload_reg);                                   // preserve the unboxed runtime argument container while branching by Mixed tag
    call_user_func_array::emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&indexed_ty),
        &indexed_label,
        emitter,
    );
    call_user_func_array::emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&assoc_ty),
        &assoc_label,
        emitter,
    );
    call_user_func_array::emit_call_user_func_array_invalid_mixed_args_abort(emitter, data);

    emitter.label(&indexed_label);
    abi::emit_load_temporary_stack_slot(emitter, mixed_reg, 0);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the borrowed unboxed indexed-array pointer before rebuilding args
    emit_receiver_prefixed_runtime_indexed_payload_arg_mixed(emitter);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&assoc_label);
    abi::emit_load_temporary_stack_slot(emitter, mixed_reg, 0);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the borrowed unboxed hash pointer before rebuilding args
    emit_receiver_prefixed_assoc_payload_arg_mixed(emitter, ctx);

    emitter.label(&done_label);
}

/// Builds a receiver-prefixed argument container from a runtime Mixed array/hash and saved receiver.
fn emit_saved_receiver_prefixed_opaque_arg_mixed(
    object_stack_offset: usize,
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("saved receiver-prefixed mixed call_user_func_array descriptor args");
    emit_push_saved_object_receiver_mixed(object_stack_offset, emitter);
    emit_receiver_prefixed_opaque_arg_mixed_after_receiver_push(arg_array, emitter, ctx, data);
}

/// Boxes a saved object pointer and preserves it as the synthetic receiver Mixed cell.
fn emit_push_saved_object_receiver_mixed(object_stack_offset: usize, emitter: &mut Emitter) {
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
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed saved receiver before evaluating the source argument container
}

/// Allocates a Mixed-valued hash sized for receiver plus the cloned source hash.
fn emit_new_receiver_prefixed_hash(source_stack_offset: usize, emitter: &mut Emitter) {
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 1);
    let source_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, source_reg, source_stack_offset);
    abi::emit_load_from_address(emitter, capacity_reg, source_reg, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #1", capacity_reg, capacity_reg)); // reserve one extra hash slot for the synthetic receiver argument
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add {}, 1", capacity_reg));           // reserve one extra hash slot for the synthetic receiver argument
        }
    }
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
}

/// Inserts the boxed receiver Mixed cell into numeric hash key zero.
fn emit_insert_receiver_hash_entry(emitter: &mut Emitter) {
    let dest_hash_off = 0;
    let receiver_off = 32;
    let hash_reg = abi::int_arg_reg_name(emitter.target, 0);
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let value_lo_reg = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi_reg = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 5);
    let stack_reg = temporary_stack_reg(emitter);

    abi::emit_load_temporary_stack_slot(emitter, hash_reg, dest_hash_off);
    abi::emit_load_int_immediate(emitter, key_ptr_reg, 0);
    abi::emit_load_int_immediate(emitter, key_len_reg, -1);
    abi::emit_load_temporary_stack_slot(emitter, value_lo_reg, receiver_off);
    abi::emit_load_int_immediate(emitter, value_hi_reg, 0);
    abi::emit_load_int_immediate(
        emitter,
        value_tag_reg,
        crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), stack_reg, dest_hash_off);
}

/// Copies cloned Mixed source entries into the destination hash, shifting numeric keys by one.
fn emit_copy_shifted_assoc_hash_entries(emitter: &mut Emitter, ctx: &mut Context) {
    let loop_label = ctx.next_label("receiver_assoc_loop");
    let done_label = ctx.next_label("receiver_assoc_done");
    let numeric_key_label = ctx.next_label("receiver_assoc_numeric_key");
    let insert_label = ctx.next_label("receiver_assoc_insert");
    let dest_hash_off = HASH_SCRATCH_BYTES;
    let source_hash_off = HASH_SCRATCH_BYTES + 16;
    let stack_reg = temporary_stack_reg(emitter);

    abi::emit_reserve_temporary_stack(emitter, HASH_SCRATCH_BYTES);
    abi::emit_load_int_immediate(emitter, abi::secondary_scratch_reg(emitter), 0);
    abi::emit_store_to_address(
        emitter,
        abi::secondary_scratch_reg(emitter),
        stack_reg,
        CURSOR_OFF,
    );

    emitter.label(&loop_label);
    emit_load_next_assoc_entry(source_hash_off, emitter);
    emit_branch_if_assoc_iteration_done(&done_label, emitter);
    emit_store_loaded_assoc_entry(emitter);
    emit_branch_if_assoc_key_is_numeric(&numeric_key_label, emitter);
    abi::emit_jump(emitter, &insert_label);

    emitter.label(&numeric_key_label);
    emit_shift_numeric_assoc_key(emitter);

    emitter.label(&insert_label);
    emit_retain_loaded_mixed_value(emitter);
    emit_insert_loaded_assoc_entry(dest_hash_off, emitter);
    abi::emit_jump(emitter, &loop_label);

    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, HASH_SCRATCH_BYTES);
}

/// Calls the hash iterator for the cloned source hash and current cursor.
fn emit_load_next_assoc_entry(source_hash_off: usize, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", source_hash_off);
            abi::emit_load_temporary_stack_slot(emitter, "x1", CURSOR_OFF);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", source_hash_off);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", CURSOR_OFF);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
        }
    }
}

/// Branches to `done_label` when the source hash iterator has reached the end.
fn emit_branch_if_assoc_iteration_done(done_label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // has the receiver-prefixed source hash scan reached the terminal cursor?
            emitter.instruction(&format!("b.eq {}", done_label));               // finish copying source entries when the hash iterator is exhausted
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // has the receiver-prefixed source hash scan reached the terminal cursor?
            emitter.instruction(&format!("je {}", done_label));                 // finish copying source entries when the hash iterator is exhausted
        }
    }
}

/// Stores the hash iterator outputs in scratch slots before nested helper calls.
fn emit_store_loaded_assoc_entry(emitter: &mut Emitter) {
    let stack_reg = temporary_stack_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_address(emitter, "x0", stack_reg, CURSOR_OFF);
            abi::emit_store_to_address(emitter, "x1", stack_reg, KEY_PTR_OFF);
            abi::emit_store_to_address(emitter, "x2", stack_reg, KEY_LEN_OFF);
            abi::emit_store_to_address(emitter, "x3", stack_reg, VALUE_LO_OFF);
            abi::emit_store_to_address(emitter, "x4", stack_reg, VALUE_HI_OFF);
            abi::emit_store_to_address(emitter, "x5", stack_reg, VALUE_TAG_OFF);
        }
        Arch::X86_64 => {
            abi::emit_store_to_address(emitter, "rax", stack_reg, CURSOR_OFF);
            abi::emit_store_to_address(emitter, "rdi", stack_reg, KEY_PTR_OFF);
            abi::emit_store_to_address(emitter, "rdx", stack_reg, KEY_LEN_OFF);
            abi::emit_store_to_address(emitter, "rcx", stack_reg, VALUE_LO_OFF);
            abi::emit_store_to_address(emitter, "r8", stack_reg, VALUE_HI_OFF);
            abi::emit_store_to_address(emitter, "r9", stack_reg, VALUE_TAG_OFF);
        }
    }
}

/// Branches to `numeric_label` when the current source key is numeric.
fn emit_branch_if_assoc_key_is_numeric(numeric_label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", KEY_LEN_OFF);
            emitter.instruction("cmn x9, #1");                                  // does the copied argument key use PHP's integer-key sentinel?
            emitter.instruction(&format!("b.eq {}", numeric_label));            // shift numeric keys so descriptor slot zero remains the receiver
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r11", KEY_LEN_OFF);
            emitter.instruction("cmp r11, -1");                                 // does the copied argument key use PHP's integer-key sentinel?
            emitter.instruction(&format!("je {}", numeric_label));              // shift numeric keys so descriptor slot zero remains the receiver
        }
    }
}

/// Rewrites a numeric source key from `n` to `n + 1` for the receiver slot.
fn emit_shift_numeric_assoc_key(emitter: &mut Emitter) {
    let stack_reg = temporary_stack_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", KEY_PTR_OFF);
            emitter.instruction("add x9, x9, #1");                              // shift a positional argument key after the synthetic receiver slot
            abi::emit_store_to_address(emitter, "x9", stack_reg, KEY_PTR_OFF);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r11", KEY_PTR_OFF);
            emitter.instruction("add r11, 1");                                  // shift a positional argument key after the synthetic receiver slot
            abi::emit_store_to_address(emitter, "r11", stack_reg, KEY_PTR_OFF);
        }
    }
}

/// Retains the loaded Mixed value cell before inserting it into the destination hash.
fn emit_retain_loaded_mixed_value(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", VALUE_LO_OFF);
            abi::emit_call_label(emitter, "__rt_incref");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rax", VALUE_LO_OFF);
            abi::emit_call_label(emitter, "__rt_incref");
        }
    }
}

/// Inserts the retained source entry into the receiver-prefixed destination hash.
fn emit_insert_loaded_assoc_entry(dest_hash_off: usize, emitter: &mut Emitter) {
    let hash_reg = abi::int_arg_reg_name(emitter.target, 0);
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let value_lo_reg = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi_reg = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 5);
    let stack_reg = temporary_stack_reg(emitter);

    abi::emit_load_temporary_stack_slot(emitter, hash_reg, dest_hash_off);
    abi::emit_load_temporary_stack_slot(emitter, key_ptr_reg, KEY_PTR_OFF);
    abi::emit_load_temporary_stack_slot(emitter, key_len_reg, KEY_LEN_OFF);
    abi::emit_load_temporary_stack_slot(emitter, value_lo_reg, VALUE_LO_OFF);
    abi::emit_load_temporary_stack_slot(emitter, value_hi_reg, VALUE_HI_OFF);
    abi::emit_load_temporary_stack_slot(emitter, value_tag_reg, VALUE_TAG_OFF);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), stack_reg, dest_hash_off);
}

/// Boxes the finished receiver-prefixed container while moving it away from the result register first.
fn emit_box_receiver_prefixed_container(
    result_reg: &str,
    container_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let container_reg = abi::nested_call_reg(emitter);
    if container_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", container_reg, result_reg)); // move the receiver-prefixed container away from the result register before Mixed boxing installs the tag
    }
    call_user_func_array::emit_box_invoker_arg_clone_as_mixed(
        container_reg,
        container_ty,
        emitter,
    );
    if container_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", result_reg, container_reg)); // return the boxed Mixed argument container through the standard expression result register
    }
}

/// Returns the target stack pointer register name for temporary stack slots.
fn temporary_stack_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "sp",
        Arch::X86_64 => "rsp",
    }
}
