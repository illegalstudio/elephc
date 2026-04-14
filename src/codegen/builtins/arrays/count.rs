use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("count()");
    let source_ty = emit_expr(&args[0], emitter, ctx, data);
    let source_repr = source_ty.codegen_repr();
    let result_reg = abi::int_result_reg(emitter);

    if source_repr.is_refcounted() && expr_result_heap_ownership(&args[0]) != HeapOwnership::Owned {
        let count_reg = abi::temp_int_reg(emitter.target);
        abi::emit_incref_if_refcounted(emitter, &source_repr);                   // retain borrowed heap-backed arrays or hashes before reading their header in place
        abi::emit_push_reg(emitter, result_reg);                                 // preserve the retained heap pointer while extracting the element count
        abi::emit_load_from_address(emitter, count_reg, result_reg, 0);          // load the element count from the first header field without consuming the retained pointer
        abi::emit_pop_reg(emitter, result_reg);                                  // restore the retained heap pointer as the decref helper argument
        abi::emit_push_reg(emitter, count_reg);                                  // preserve the computed count across the decref helper call
        abi::emit_decref_if_refcounted(emitter, &source_repr);                   // release the temporary owner once the header count has been captured
        abi::emit_pop_reg(emitter, result_reg);                                  // restore the computed count as the builtin integer result
    } else {
        // -- read element count from array/hash header --
        abi::emit_load_from_address(emitter, result_reg, result_reg, 0);         // load element count directly when the current expression already owns its heap payload
    }

    Some(PhpType::Int)
}
