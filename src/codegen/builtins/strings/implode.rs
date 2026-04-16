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
    emitter.comment("implode()");
    // implode($glue, $array)
    emit_expr(&args[0], emitter, ctx, data);
    // -- save glue, evaluate array --
    let (glue_ptr_reg, glue_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, glue_ptr_reg, glue_len_reg);               // preserve the glue string while evaluating the indexed array argument
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);
    // -- save array pointer, restore glue --
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the indexed array pointer while restoring the glue string for the runtime call

    let is_int_array = matches!(&arr_ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Int | PhpType::Bool));

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x3");                                   // restore the indexed array pointer into the runtime array-argument register
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // restore the glue string into the runtime string-argument registers
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "rdx");                                  // restore the indexed array pointer into the third SysV integer argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the glue string into the first two SysV integer argument registers
        }
    }

    if is_int_array {
        abi::emit_call_label(emitter, "__rt_implode_int");                      // join integer array elements with the glue string through the integer-specialized runtime
    } else {
        abi::emit_call_label(emitter, "__rt_implode");                          // join string array elements with the glue string through the standard runtime
    }

    Some(PhpType::Str)
}
