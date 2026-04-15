use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
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
    emitter.comment("array_merge()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the first scalar indexed-array pointer while evaluating the second merge operand
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // move the second scalar indexed-array pointer into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the first scalar indexed-array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_merge_refcounted");       // merge the two refcounted indexed arrays through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_merge");                  // merge the two scalar indexed arrays through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- save first array, evaluate second array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push first array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to merge two arrays --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    if uses_refcounted_runtime {
        emitter.instruction("bl __rt_array_merge_refcounted");                  // merge arrays while retaining borrowed heap elements
    } else {
        emitter.instruction("bl __rt_array_merge");                             // call runtime: merge arrays → x0=new array
    }

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
