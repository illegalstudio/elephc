//! Purpose:
//! Emits PHP `count` calls for arrays and countable runtime values.
//! Loads lengths from typed array layouts or boxed runtime structures as needed.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Ownership state is observed but count must not consume or mutate the counted value.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::codegen::expr::objects::dispatch::emit_dispatch_instance_method;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
use crate::codegen::platform::Arch;
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

    if let PhpType::Object(class_name) = &source_ty {
        if class_implements_countable(class_name, ctx) {
            if emitter.target.arch == Arch::X86_64 {
                emitter.instruction("mov rdi, rax");                            // forward Countable receiver to first arg slot
            }
            emit_dispatch_instance_method(class_name, "count", emitter, ctx);
            return Some(PhpType::Int);
        }
    }

    let source_repr = source_ty.codegen_repr();
    let result_reg = abi::int_result_reg(emitter);

    if matches!(source_repr, PhpType::Mixed) {
        // Mixed receivers: unbox the cell at runtime and dispatch to the
        // shared count helper, which reads the array/hash header. Returns
        // 0 when the boxed payload is not a container, mirroring PHP's
        // long-standing "count(): Argument is not countable" warning
        // behavior collapsed to a quiet zero for the most common idiom
        // (count(json_decode($json, true))).
        abi::emit_call_label(emitter, "__rt_mixed_count");                       // unbox the Mixed cell and read the array/hash count from its payload header
        return Some(PhpType::Int);
    }

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

fn class_implements_countable(class_name: &str, ctx: &Context) -> bool {
    ctx.classes
        .get(class_name)
        .map(|info| info.interfaces.iter().any(|i| i == "Countable"))
        .unwrap_or(false)
}
