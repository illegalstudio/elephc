//! Purpose:
//! Emits PHP `unset` calls that clear variables or array elements.
//! Coordinates ownership cleanup with caller storage updates for removed values.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Unset is mutating and must release owned refcounted values without touching unrelated aliases.

use crate::codegen::abi;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("unset()");
    for arg in args {
        emit_unset_arg(arg, emitter, ctx, data);
    }
    Some(PhpType::Void)
}

fn emit_unset_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if let crate::parser::ast::ExprKind::ArrayAccess { array, index } = &arg.kind {
        if crate::codegen::expr::arrays::type_is_array_access_object(
            &crate::codegen::functions::infer_contextual_type(array, ctx),
            ctx,
        ) {
            crate::codegen::expr::arrays::emit_array_access_offset_unset(
                array, index, emitter, ctx, data,
            );
            return;
        }
    }

    if let crate::parser::ast::ExprKind::Variable(name) = &arg.kind {
        let var = ctx.variables.get(name).expect("undefined variable");
        let offset = var.stack_offset;
        let old_ty = var.ty.clone();

        // -- free old heap value before unsetting --
        if matches!(&old_ty, PhpType::Str) {
            abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset); // load the previous heap pointer from the variable slot
            abi::emit_call_label(emitter, "__rt_heap_free_safe");               // free old string storage when the previous value is heap-backed
        } else if matches!(&old_ty, PhpType::Array(_)) {
            abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset); // load the previous heap pointer from the variable slot
            abi::emit_call_label(emitter, "__rt_decref_array");                 // decrement the array refcount and deep-free when it reaches zero
        } else if matches!(&old_ty, PhpType::AssocArray { .. }) {
            abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset); // load the previous heap pointer from the variable slot
            abi::emit_call_label(emitter, "__rt_decref_hash");                  // decrement the hash refcount and deep-free when it reaches zero
        } else if matches!(&old_ty, PhpType::Object(_)) {
            abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset); // load the previous heap pointer from the variable slot
            abi::emit_call_label(emitter, "__rt_decref_object");                // decrement the object refcount and deep-free when it reaches zero
        }

        // -- set variable to null sentinel value (0x7FFFFFFFFFFFFFFFE) --
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::MAX - 1); // materialize the shared null sentinel in the target integer result register
        abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);     // store the null sentinel back into the variable slot
        ctx.update_var_type_and_ownership(name, PhpType::Void, HeapOwnership::NonHeap);
    }
}
