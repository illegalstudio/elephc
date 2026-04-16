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
    emitter.comment("array_diff()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        let uses_refcounted_runtime =
            matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
        abi::emit_push_reg(emitter, "rax");                                     // preserve the first input array while evaluating the second input array expression
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // place the second input array pointer in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the first input array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_diff_refcounted");        // compute the borrowed-heap-aware array difference through the dedicated x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_diff");                   // compute the integer array difference through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    // -- save first array, evaluate second array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push first array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to compute value difference --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_diff_refcounted"
    } else {
        "bl __rt_array_diff"
    };
    emitter.instruction(runtime_call);                                          // call runtime: diff arrays → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
