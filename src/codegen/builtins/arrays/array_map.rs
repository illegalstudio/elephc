use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::array_map_callback_returns_str::callback_returns_str;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_map()");
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);

    // -- determine callback return type at compile time --
    let returns_str = callback_returns_str(args, ctx);

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    if is_closure {
        emit_expr(&args[0], emitter, ctx, data);
        abi::emit_push_reg(emitter, result_reg);                                // save the synthesized callback address on the temporary stack
    }

    // -- evaluate the array argument --
    let _arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // -- save array pointer, load callback address into the target nested-call scratch register --
    abi::emit_push_reg(emitter, result_reg);                                    // push the array pointer onto the temporary stack

    if is_closure {
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, call_reg, offset);                          // load the callback address from the callable variable slot
    } else {
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_map() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        abi::emit_symbol_address(emitter, call_reg, &label);                         // materialize the callback function address in the nested-call scratch register
    }

    if is_closure {
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the mapped array pointer into the second runtime argument register
        abi::emit_pop_reg(emitter, callback_arg_reg);                            // pop the synthesized callback address into the first runtime argument register
    } else {
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the mapped array pointer into the second runtime argument register
        emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg)); // move the callback function address into the first runtime argument register
    }

    if returns_str {
        abi::emit_call_label(emitter, "__rt_array_map_str");                    // call the string-producing array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Str)))
    } else {
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Int)))
    }
}
