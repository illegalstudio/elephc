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
    emitter.comment("array_shift()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emit_ensure_unique_arg(emitter, &arr_ty);
        emit_store_mutating_arg(emitter, ctx, &args[0]);
        emitter.instruction("mov rdi, rax");                                    // move the unique indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_shift");                      // remove and return the first scalar indexed-array element through the x86_64 runtime helper
        return match arr_ty {
            PhpType::Array(inner) => Some(*inner),
            _ => Some(PhpType::Int),
        };
    }

    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- call runtime to remove and return first element --
    emitter.instruction("bl __rt_array_shift");                                 // call runtime: shift first element → x0=removed element

    match arr_ty {
        PhpType::Array(inner) => Some(*inner),
        _ => Some(PhpType::Int),
    }
}
