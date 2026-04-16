use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the indexed array of keys while evaluating the indexed array of values expression
        let values_ty = emit_expr(&args[1], emitter, ctx, data);
        let (key_elem_ty, value_elem_ty) = match (&keys_ty, &values_ty) {
            (PhpType::Array(key), PhpType::Array(value)) => ((**key).clone(), (**value).clone()),
            _ => (PhpType::Str, PhpType::Int),
        };
        let uses_refcounted_runtime = value_elem_ty.is_refcounted();
        let value_type_tag = hash_value_type_tag(&value_elem_ty);
        if !uses_refcounted_runtime {
            abi::emit_load_int_immediate(emitter, "rdx", value_type_tag.into());
            emitter.instruction("mov rsi, rax");                                // place the indexed array of values in the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the indexed array of keys into the first x86_64 runtime argument register
            abi::emit_call_label(emitter, "__rt_array_combine");                // build the scalar associative array through the x86_64 runtime helper
        } else {
            emitter.instruction("mov rcx, rax");                                // preserve the indexed array of values while materializing the result hash value_type tag for the refcounted helper path
            abi::emit_load_int_immediate(emitter, "rdx", value_type_tag.into());
            emitter.instruction("mov rsi, rcx");                                // place the indexed array of values in the second x86_64 runtime argument register for the refcounted helper path
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the indexed array of keys into the first x86_64 runtime argument register for the refcounted helper path
            abi::emit_call_label(emitter, "__rt_array_combine_refcounted");     // build the refcounted associative array through the dedicated x86_64 runtime helper
        }

        return Some(PhpType::AssocArray {
            key: Box::new(key_elem_ty),
            value: Box::new(value_elem_ty),
        });
    }

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
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_combine_refcounted"
    } else {
        "bl __rt_array_combine"
    };
    emitter.instruction(runtime_call);                                          // call runtime: combine → x0=new assoc array

    Some(PhpType::AssocArray {
        key: Box::new(key_elem_ty),
        value: Box::new(value_elem_ty),
    })
}
