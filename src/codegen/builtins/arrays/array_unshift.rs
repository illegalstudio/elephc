use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
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
    emitter.comment("array_unshift()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emit_ensure_unique_arg(emitter, &arr_ty);
        emit_store_mutating_arg(emitter, ctx, &args[0]);
        abi::emit_push_reg(emitter, "rax");                                     // preserve the unique indexed-array pointer while evaluating the prepended scalar payload
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // move the prepended scalar payload into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the unique indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_unshift");                    // prepend the scalar payload through the x86_64 runtime helper and return the new length
        return Some(PhpType::Int);
    }

    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- save array pointer, evaluate value to prepend --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to prepend value to array --
    emitter.instruction("mov x1, x0");                                          // move value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    emitter.instruction("bl __rt_array_unshift");                               // call runtime: prepend value → x0=new count

    Some(PhpType::Int)
}
