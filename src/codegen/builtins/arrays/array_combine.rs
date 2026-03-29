use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::hash_value_type_tag::hash_value_type_tag;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_combine()");
    let keys_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save keys array, evaluate values array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push keys array pointer onto stack
    let values_ty = emit_expr(&args[1], emitter, ctx, data);
    let (key_elem_ty, value_elem_ty) = match (&keys_ty, &values_ty) {
        (PhpType::Array(key), PhpType::Array(value)) => ((**key).clone(), (**value).clone()),
        _ => (PhpType::Str, PhpType::Int),
    };
    let uses_refcounted_runtime = value_elem_ty.is_refcounted();
    let value_type_tag = hash_value_type_tag(&value_elem_ty);
    // -- call runtime to combine keys and values into assoc array --
    emitter.instruction(&format!("mov x2, #{}", value_type_tag));               // x2 = result hash value_type tag
    emitter.instruction("mov x1, x0");                                          // move values array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop keys array pointer into x0
    emitter.instruction(if uses_refcounted_runtime {
        "bl __rt_array_combine_refcounted"
    } else {
        "bl __rt_array_combine"
    }); // call runtime: combine → x0=new assoc array

    Some(PhpType::AssocArray {
        key: Box::new(key_elem_ty),
        value: Box::new(value_elem_ty),
    })
}
