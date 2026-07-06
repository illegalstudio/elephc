//! Purpose:
//! Dispatches array element assignment to associative, indexed, buffer, and specialized paths.
//! Chooses the storage strategy from the target expression and inferred container type.
//!
//! Called from:
//! - `crate::codegen_support::stmt::arrays`
//!
//! Key details:
//! - Write paths must evaluate targets and values once while preserving container ownership updates.

mod assoc;
mod buffer;
mod indexed;

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::stmt::helpers;
use crate::codegen_support::{abi, emit_box_current_expr_value_as_mixed_for_container};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits `$array[index] = value` for a named array variable.
/// Handles null-coalesce assignment (`??=`), `ArrayAccess` objects, `Mixed` types,
/// buffers, associative arrays, and indexed arrays. Preserves COW semantics and
/// evaluates the index and value expressions exactly once.
pub(super) fn emit_array_assign_stmt(
    array: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${}[...] = ...", array));
    if let Some((current, default)) = super::super::null_coalesce_array_target(array, index, value)
    {
        if matches!(default.kind, ExprKind::Null) {
            emitter.comment("literal null fallback leaves the array slot unchanged");
            return;
        }
        let current_ty = crate::codegen_support::expr::emit_expr(current, emitter, ctx, data);
        if current_ty != PhpType::Void {
            let keep_label = ctx.next_label("nca_keep");
            super::super::emit_branch_if_result_non_null(&current_ty, &keep_label, emitter);
            emit_array_assign_stmt(array, index, default, emitter, ctx, data);
            emitter.label(&keep_label);
        } else {
            emit_array_assign_stmt(array, index, default, emitter, ctx, data);
        }
        return;
    }

    let var = match ctx.variables.get(array) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined variable ${}", array));
            return;
        }
    };
    let var_ty = var.ty.clone();
    let var_static_ty = var.static_ty.clone();
    let offset = var.stack_offset;
    let is_ref = ctx.ref_params.contains(array);
    if crate::codegen_support::expr::arrays::type_is_array_access_object(&var_static_ty, ctx)
        || crate::codegen_support::expr::arrays::type_is_array_access_object(&var_ty, ctx)
    {
        let object = Expr::new(ExprKind::Variable(array.to_string()), index.span);
        crate::codegen_support::expr::arrays::emit_array_access_offset_set(
            &object, index, value, emitter, ctx, data,
        );
        return;
    }
    if matches!(var_ty, PhpType::Mixed) {
        let result_reg = abi::int_result_reg(emitter);
        let ref_reg = abi::symbol_scratch_reg(emitter);
        if is_ref {
            abi::load_at_offset(emitter, ref_reg, offset);                      // load the by-reference slot that points at the Mixed local
            abi::emit_load_from_address(emitter, result_reg, ref_reg, 0);       // dereference the by-reference slot to get the current Mixed cell
        } else {
            abi::load_at_offset(emitter, result_reg, offset);                   // load the current Mixed cell pointer from the local slot
        }
        emit_mixed_array_assign_with_loaded_base(index, value, emitter, ctx, data);
        return;
    }
    let target = ArrayAssignTarget {
        array,
        offset,
        is_ref,
        elem_ty: match &var_ty {
            PhpType::Array(t) => *t.clone(),
            PhpType::AssocArray { value: v, .. } => *v.clone(),
            PhpType::Buffer(t) => *t.clone(),
            _ => PhpType::Int,
        },
    };

    match &var_ty {
        PhpType::Buffer(_) => {
            buffer::emit_buffer_array_assign(&target, index, value, emitter, ctx, data);
        }
        PhpType::AssocArray { .. } => {
            assoc::emit_assoc_array_assign(&target, index, value, emitter, ctx, data);
        }
        _ => {
            indexed::emit_indexed_array_assign(&target, index, value, emitter, ctx, data);
        }
    }
}

/// Emits a nested array-element assignment where the target is an `ArrayAccess` expression
/// (`$array[index][sub_index] = value`). Routes to `ArrayAccess` object setters or `Mixed`
/// array setters based on the base expression's inferred type.
pub(super) fn emit_nested_array_assign_stmt(
    target: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("[...][...] = ...");
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        emitter.comment("WARNING: nested array assignment requires an array-access target");
        return;
    };

    let base_static_ty = crate::codegen_support::functions::infer_contextual_type(array, ctx);
    if crate::codegen_support::expr::arrays::type_is_array_access_object(&base_static_ty, ctx) {
        crate::codegen_support::expr::arrays::emit_array_access_offset_set(
            array, index, value, emitter, ctx, data,
        );
        return;
    }

    let base_ty = crate::codegen_support::expr::emit_expr(array, emitter, ctx, data);
    if matches!(base_ty, PhpType::Mixed) {
        emit_mixed_array_assign_with_loaded_base(index, value, emitter, ctx, data);
    } else {
        emitter.comment("WARNING: nested array assignment requires a Mixed target");
    }
}

/// Shared lowering for `Mixed` array element assignment when the base array pointer is
/// already loaded into the result register. Pushes the base, emits the normalized key,
/// emits the RHS and boxes it as `Mixed` if needed, then calls `__rt_mixed_array_set`
/// to mutate the slot in place. Clobbers `x0`/`rax`, `x1`/`rsi`, `x2`/`rdx`, `x3`/`rcx`.
fn emit_mixed_array_assign_with_loaded_base(
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed Mixed array cell while key and RHS expressions run
    crate::codegen_support::emit_normalized_hash_key(index, emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2");                      // preserve the normalized key tuple until the RHS is boxed
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                    // preserve the normalized key tuple until the RHS is boxed
        }
    }

    let val_ty = crate::codegen_support::expr::emit_expr(value, emitter, ctx, data);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    } else {
        emit_box_current_expr_value_as_mixed_for_container(emitter, value, &val_ty);
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x0");                                  // pass the boxed RHS as the consumed Mixed value argument
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                       // restore the normalized array key for the runtime setter
            abi::emit_pop_reg(emitter, "x0");                                   // restore the target Mixed array cell for the runtime setter
            emitter.instruction("bl __rt_mixed_array_set");                     // mutate the decoded Mixed array or hash slot in place
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // pass the boxed RHS as the consumed Mixed value argument
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                     // restore the normalized array key for the runtime setter
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the target Mixed array cell for the runtime setter
            emitter.instruction("call __rt_mixed_array_set");                   // mutate the decoded Mixed array or hash slot in place
        }
    }
}

#[derive(Clone)]
/// Carries the layout and type metadata needed to emit array-element assignment for a
/// named variable. `offset` is the stack slot; `is_ref` indicates by-reference binding;
/// `elem_ty` is the PHP element type of the container.
pub(super) struct ArrayAssignTarget<'a> {
    /// Name of the PHP array variable being assigned to.
    pub array: &'a str,
    /// Stack frame offset (in bytes) of the array variable's storage slot.
    pub offset: usize,
    /// True when the variable is bound by-reference (`&$array`) and holds a pointer to
    /// the actual array storage rather than the storage directly.
    pub is_ref: bool,
    /// Inferred PHP element type of the container (e.g., `PhpType::Int` for a vector).
    /// Used to select the correct storage strategy and runtime helper.
    pub elem_ty: PhpType,
}
