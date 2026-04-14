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
    emitter.comment("array_pad()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 && !uses_refcounted_runtime {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the source scalar indexed-array pointer while evaluating the target size expression
        emit_expr(&args[1], emitter, ctx, data);
        abi::emit_push_reg(emitter, "rax");                                     // preserve the requested target size while evaluating the scalar pad value
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("mov rdx, rax");                                    // move the scalar pad value into the third x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rsi");                                      // restore the requested target size into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the source scalar indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_pad");                        // pad the scalar indexed array through the x86_64 runtime helper

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- save array pointer, evaluate target size --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- save target size, evaluate pad value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push target size onto stack
    emit_expr(&args[2], emitter, ctx, data);
    // -- set up three-arg call: array, size, value --
    emitter.instruction("mov x2, x0");                                          // move pad value to x2 (third arg)
    emitter.instruction("ldr x1, [sp], #16");                                   // pop target size into x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_pad_refcounted"
    } else {
        "bl __rt_array_pad"
    };
    emitter.instruction(runtime_call);                                          // call runtime: pad array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
