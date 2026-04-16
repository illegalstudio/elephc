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
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("unset()");
    if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
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
    Some(PhpType::Void)
}
