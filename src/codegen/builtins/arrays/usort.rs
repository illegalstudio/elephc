use crate::codegen::abi;
use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("usort()");

    // -- evaluate the array argument (first arg) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);

    // -- save array pointer --
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the array pointer while the callback address is resolved for the target ABI

    // -- resolve callback function address --
    let is_closure = matches!(
        &args[1].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    if is_closure {
        emit_expr(&args[1], emitter, ctx, data);
        match emitter.target.arch {
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the resolved closure callback address into the first SysV usort() runtime argument register
            }
            Arch::AArch64 => {
                emitter.instruction("mov x0, x0");                              // keep the resolved closure callback address in the first AArch64 usort() runtime argument register
            }
        }
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        match emitter.target.arch {
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "rdi", offset);                    // load the callback address from the variable slot into the first SysV usort() runtime argument register
            }
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x0", offset);                     // load the callback address from the variable slot into the first AArch64 usort() runtime argument register
            }
        }
    } else {
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("usort() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        match emitter.target.arch {
            Arch::X86_64 => {
                abi::emit_symbol_address(emitter, "rdi", &label);               // materialize the comparator function address in the first SysV usort() runtime argument register
            }
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x0", &label);                // materialize the comparator function address in the first AArch64 usort() runtime argument register
            }
        }
    }

    // -- call runtime: callback_addr + array_ptr --
    match emitter.target.arch {
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the array pointer into the second SysV usort() runtime argument register
        }
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x1");                                   // restore the array pointer into the second AArch64 usort() runtime argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_usort");                                // call the target-aware runtime helper that sorts the indexed array using the comparator callback

    Some(PhpType::Void)
}
