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
    emitter.comment("array_diff_key()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save first array, evaluate second array --
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the first associative-array pointer while evaluating the mask array
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the second associative-array pointer into the second runtime helper argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the first associative-array pointer into the first runtime helper argument register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the second associative-array pointer into the second SysV runtime helper argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the first associative-array pointer into the first SysV runtime helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_array_diff_key");                       // compute the associative-array key difference and return the filtered hash table pointer

    Some(arr_ty)
}
