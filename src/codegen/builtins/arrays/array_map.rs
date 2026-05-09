use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::array_map_callback_returns_str::callback_returns_str;
use super::callback_env;

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
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);

    // -- determine callback return type at compile time --
    let returns_str = callback_returns_str(args, ctx);
    let source_elem_ty = match crate::codegen::functions::infer_contextual_type(&args[1], ctx) {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    };

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let mut inline_captures = Vec::new();
    if is_closure {
        emit_expr(&args[0], emitter, ctx, data);
        inline_captures = callback_env::callback_captures(&args[0], ctx);
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
    let captures = if is_closure {
        inline_captures
    } else {
        callback_env::callback_captures(&args[0], ctx)
    };

    if is_closure {
        if captures.is_empty() {
            abi::emit_pop_reg(emitter, array_arg_reg);                           // pop the mapped array pointer into the second runtime argument register
            abi::emit_pop_reg(emitter, callback_arg_reg);                        // pop the synthesized callback address into the first runtime argument register
            abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
        } else {
            abi::emit_pop_reg(emitter, result_reg);                              // recover the mapped array pointer before building the capture environment
            abi::emit_pop_reg(emitter, call_reg);                                // recover the original closure entry point for env slot zero
            let wrapper = callback_env::emit_captured_callback_env(
                call_reg,
                result_reg,
                &captures,
                vec![source_elem_ty.clone()],
                emitter,
                ctx,
            );
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            if returns_str {
                abi::emit_call_label(emitter, "__rt_array_map_str");            // call the string-producing array_map runtime helper
                abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
                return Some(PhpType::Array(Box::new(PhpType::Str)));
            }
            abi::emit_call_label(emitter, "__rt_array_map");                    // call the scalar array_map runtime helper
            abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
            return Some(PhpType::Array(Box::new(PhpType::Int)));
        }
    } else {
        if captures.is_empty() {
            abi::emit_pop_reg(emitter, array_arg_reg);                           // pop the mapped array pointer into the second runtime argument register
            emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg)); // move the callback function address into the first runtime argument register
            abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
        } else {
            abi::emit_pop_reg(emitter, result_reg);                              // recover the mapped array pointer before building the capture environment
            let wrapper = callback_env::emit_captured_callback_env(
                call_reg,
                result_reg,
                &captures,
                vec![source_elem_ty.clone()],
                emitter,
                ctx,
            );
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            if returns_str {
                abi::emit_call_label(emitter, "__rt_array_map_str");            // call the string-producing array_map runtime helper
                abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
                return Some(PhpType::Array(Box::new(PhpType::Str)));
            }
            abi::emit_call_label(emitter, "__rt_array_map");                    // call the scalar array_map runtime helper
            abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
            return Some(PhpType::Array(Box::new(PhpType::Int)));
        }
    }

    if returns_str {
        abi::emit_call_label(emitter, "__rt_array_map_str");                    // call the string-producing array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Str)))
    } else {
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Int)))
    }
}
