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
    emitter.comment("array_chunk()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64
        && !matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted())
    {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the source indexed array while evaluating the requested chunk size expression
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // place the requested chunk size in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the source indexed array into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_chunk");                      // split the scalar indexed array into chunk arrays through the x86_64 runtime helper

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(Box::new(PhpType::Array(inner)))),
            _ => Some(PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Int))))),
        };
    }

    // -- save array pointer, evaluate chunk size --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to split array into chunks --
    emitter.instruction("mov x1, x0");                                          // move chunk size to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    if matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted()) {
        emitter.instruction("bl __rt_array_chunk_refcounted");                  // chunk array while retaining borrowed heap elements
    } else {
        emitter.instruction("bl __rt_array_chunk");                             // call runtime: chunk array → x0=array of arrays
    }

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(Box::new(PhpType::Array(inner)))),
        _ => Some(PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Int))))),
    }
}
