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
    emitter.comment("array_fill_keys()");
    let keys_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save keys array, evaluate fill value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push keys array pointer onto stack
    let value_ty = emit_expr(&args[1], emitter, ctx, data);
    let key_elem_ty = match &keys_ty {
        PhpType::Array(key) => (**key).clone(),
        _ => PhpType::Str,
    };
    let uses_refcounted_runtime = value_ty.is_refcounted();
    let value_type_tag = hash_value_type_tag(&value_ty);
    // -- call runtime to create assoc array from keys with given value --
    emitter.instruction(&format!("mov x2, #{}", value_type_tag));               // x2 = result hash value_type tag
    emitter.instruction("mov x1, x0");                                          // move fill value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop keys array pointer into x0 (first arg)
    emitter.instruction(if uses_refcounted_runtime {
        "bl __rt_array_fill_keys_refcounted"
    } else {
        "bl __rt_array_fill_keys"
    }); // call runtime: fill keys → x0=new assoc array

    Some(PhpType::AssocArray {
        key: Box::new(key_elem_ty),
        value: Box::new(value_ty),
    })
}
