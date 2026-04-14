use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_slice()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 && !uses_refcounted_runtime {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the source indexed-array pointer while evaluating the slice offset
        emit_expr(&args[1], emitter, ctx, data);
        if args.len() > 2 {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the requested slice offset while evaluating the slice length
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov rdx, rax");                                // move the requested slice length into the third x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the requested slice offset into the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the source indexed-array pointer into the first x86_64 runtime argument register
        } else {
            emitter.instruction("mov rsi, rax");                                // move the requested slice offset into the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the source indexed-array pointer into the first x86_64 runtime argument register
            emitter.instruction("mov rdx, -1");                                 // use -1 as the x86_64 runtime sentinel for slicing until the end of the source array
        }
        abi::emit_call_label(emitter, "__rt_array_slice");                      // extract the scalar indexed-array slice through the x86_64 runtime helper

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- save array pointer, evaluate offset --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    if args.len() > 2 {
        // -- save offset, evaluate length --
        emitter.instruction("str x0, [sp, #-16]!");                             // push offset onto stack
        emit_expr(&args[2], emitter, ctx, data);
        // -- set up three-arg call: array, offset, length --
        emitter.instruction("mov x2, x0");                                      // move length to x2 (third arg)
        emitter.instruction("ldr x1, [sp], #16");                               // pop offset into x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
    } else {
        // -- set up two-arg call: array, offset (length = rest of array) --
        emitter.instruction("mov x1, x0");                                      // move offset to x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
        emitter.instruction("mov x2, #-1");                                     // length = -1 signals "until end of array"
    }
    // -- call runtime to extract slice --
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_slice_refcounted"
    } else {
        "bl __rt_array_slice"
    };
    emitter.instruction(runtime_call);                                          // call runtime: slice array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
