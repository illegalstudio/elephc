use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::callback_env;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_filter()");
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(
        &args[1].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let mut inline_captures = Vec::new();
    if is_closure {
        emit_expr(&args[1], emitter, ctx, data);
        inline_captures = callback_env::callback_captures(&args[1], ctx);
        abi::emit_push_reg(emitter, result_reg);                                // save the synthesized callback address on the temporary stack
    }

    // -- evaluate the array argument (first arg) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());

    // -- save array pointer, then resolve the callback address into the target scratch register --
    abi::emit_push_reg(emitter, result_reg);                                    // push the source array pointer onto the temporary stack

    if is_closure {
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, call_reg, offset);                         // load the callback address from the callable variable slot
    } else {
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_filter() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        abi::emit_symbol_address(emitter, call_reg, &label);                         // materialize the callback function address in the nested-call scratch register
    }
    let captures = if is_closure {
        inline_captures
    } else {
        callback_env::callback_captures(&args[1], ctx)
    };

    // -- place callback and array pointer into the runtime argument registers --
    if is_closure {
        if captures.is_empty() {
            abi::emit_pop_reg(emitter, array_arg_reg);                           // pop the source array pointer into the second runtime argument register
            abi::emit_pop_reg(emitter, callback_arg_reg);                        // pop the synthesized callback address into the first runtime argument register
            abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
        } else {
            abi::emit_pop_reg(emitter, result_reg);                              // recover the source array pointer before building the capture environment
            abi::emit_pop_reg(emitter, call_reg);                                // recover the original closure entry point for env slot zero
            let wrapper = callback_env::emit_captured_callback_env(
                call_reg,
                result_reg,
                &captures,
                vec![filter_elem_type(&arr_ty)],
                emitter,
                ctx,
            );
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            let runtime_label = if uses_refcounted_runtime {
                "__rt_array_filter_refcounted"
            } else {
                "__rt_array_filter"
            };
            abi::emit_call_label(emitter, runtime_label);
            abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
            return match arr_ty {
                PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
                _ => Some(PhpType::Array(Box::new(PhpType::Int))),
            };
        }
    } else {
        if captures.is_empty() {
            abi::emit_pop_reg(emitter, array_arg_reg);                           // pop the source array pointer into the second runtime argument register
            emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg)); // move the callback function address into the first runtime argument register
            abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
        } else {
            abi::emit_pop_reg(emitter, result_reg);                              // recover the source array pointer before building the capture environment
            let wrapper = callback_env::emit_captured_callback_env(
                call_reg,
                result_reg,
                &captures,
                vec![filter_elem_type(&arr_ty)],
                emitter,
                ctx,
            );
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            let runtime_label = if uses_refcounted_runtime {
                "__rt_array_filter_refcounted"
            } else {
                "__rt_array_filter"
            };
            abi::emit_call_label(emitter, runtime_label);
            abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
            return match arr_ty {
                PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
                _ => Some(PhpType::Array(Box::new(PhpType::Int))),
            };
        }
    }

    let runtime_label = if uses_refcounted_runtime {
        "__rt_array_filter_refcounted"
    } else {
        "__rt_array_filter"
    };
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_call_label(emitter, runtime_label);                            // call the x86_64 callback-driven filter runtime helper
    } else {
        emitter.instruction(&format!("bl {}", runtime_label));                  // call the ARM64 callback-driven filter runtime helper
    }

    match arr_ty {
        PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}

fn filter_elem_type(arr_ty: &PhpType) -> PhpType {
    match arr_ty {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    }
}
