use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

pub(super) fn emit_expr_call(
    callee: &Expr,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call expression result");

    let callee_sig = match &callee.kind {
        ExprKind::Variable(var_name) => ctx.closure_sigs.get(var_name).cloned(),
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(arr_name) = &array.kind {
                ctx.closure_sigs.get(arr_name).cloned()
            } else {
                None
            }
        }
        ExprKind::FirstClassCallable(target) => super::first_class_callable_sig(target, ctx),
        _ => None,
    };

    let is_variadic = callee_sig.as_ref().map(|s| s.variadic.is_some()).unwrap_or(false);
    let regular_param_count = callee_sig
        .as_ref()
        .map(|s| {
            if s.variadic.is_some() {
                s.params.len().saturating_sub(1)
            } else {
                s.params.len()
            }
        })
        .unwrap_or(args_exprs.len());
    let normalized_args = callee_sig
        .as_ref()
        .map(|sig| args::normalize_named_call_args(sig, args_exprs, regular_param_count))
        .unwrap_or_else(|| args_exprs.to_vec());
    let args_exprs = normalized_args.as_slice();
    let mut regular_args: Vec<&Expr> = Vec::new();
    let mut variadic_args: Vec<&Expr> = Vec::new();
    let mut spread_arg: Option<&Expr> = None;
    let mut spread_at_index: usize = 0;
    for (i, arg) in args_exprs.iter().enumerate() {
        if let ExprKind::Spread(inner) = &arg.kind {
            spread_arg = Some(inner.as_ref());
            spread_at_index = regular_args.len();
        } else if is_variadic && i >= regular_param_count {
            variadic_args.push(arg);
        } else {
            regular_args.push(arg);
        }
    }
    let spread_into_named = spread_arg.is_some() && !is_variadic;

    let mut all_args: Vec<&Expr> = regular_args;
    let mut default_exprs: Vec<Expr> = Vec::new();
    if !spread_into_named {
        if let Some(ref sig) = callee_sig {
            for i in all_args.len()..regular_param_count {
                if let Some(Some(default)) = sig.defaults.get(i) {
                    default_exprs.push(default.clone());
                }
            }
        }
        let default_refs: Vec<&Expr> = default_exprs.iter().collect();
        all_args.extend(default_refs);
    }

    let mut arg_types = Vec::new();
    for (i, arg) in all_args.iter().enumerate() {
        let is_ref = callee_sig
            .as_ref()
            .and_then(|sig| sig.ref_params.get(i))
            .copied()
            .unwrap_or(false);
        let target_ty = args::declared_target_ty(callee_sig.as_ref(), i);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if ctx.global_vars.contains(var_name) {
                    let label = format!("_gvar_{}", var_name);
                    emitter.comment(&format!("indirect ref arg: address of global ${}", var_name));
                    emitter.adrp("x0", &format!("{}", label));   // load page of global var
                    emitter.add_lo12("x0", "x0", &format!("{}", label)); //resolve global var address
                } else if ctx.ref_params.contains(var_name) {
                    let Some(var) = ctx.variables.get(var_name) else {
                        emitter.comment(&format!("WARNING: undefined ref variable ${}", var_name));
                        continue;
                    };
                    emitter.comment(&format!("indirect ref arg: forward underlying reference for ${}", var_name));
                    crate::codegen::abi::load_at_offset(emitter, "x0", var.stack_offset); // load existing reference pointer
                } else {
                    let Some(var) = ctx.variables.get(var_name) else {
                        emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                        continue;
                    };
                    emitter.comment(&format!("indirect ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", var.stack_offset)); //compute address of local variable
                }
            } else {
                let ty = super::super::emit_expr(arg, emitter, ctx, data);
                super::super::retain_borrowed_heap_arg(emitter, arg, &ty);
            }
            emitter.instruction("str x0, [sp, #-16]!");                         // push address for by-ref argument
            arg_types.push(PhpType::Int);
        } else {
            let pushed_ty = args::push_expr_arg(arg, target_ty, emitter, ctx, data);
            arg_types.push(pushed_ty);
        }
    }

    if spread_into_named {
        if let Some(spread_expr) = spread_arg {
            let remaining = regular_param_count.saturating_sub(spread_at_index);
            emitter.comment(&format!("unpack spread into {} indirect params", remaining));
            let spread_ty = functions::infer_contextual_type(spread_expr, ctx);
            let source_elem_ty = match &spread_ty {
                PhpType::Array(elem) => (**elem).clone(),
                PhpType::AssocArray { value, .. } => (**value).clone(),
                _ => PhpType::Int,
            };
            let elem_stride = args::array_element_stride(&source_elem_ty);
            let _ = super::super::emit_expr(spread_expr, emitter, ctx, data);
            emitter.instruction("mov x20, x0");                                 // preserve the spread array pointer across boxing/incref helper calls
            emitter.instruction("add x20, x20, #24");                           // skip 24-byte array header to reach data
            for idx in 0..remaining {
                let target_ty = args::declared_target_ty(callee_sig.as_ref(), spread_at_index + idx);
                args::load_array_element_to_result(emitter, &source_elem_ty, "x20", idx * elem_stride);
                let pushed_ty =
                    args::push_loaded_array_element_arg(&source_elem_ty, target_ty, emitter, ctx, data);
                arg_types.push(pushed_ty);
            }
        }
    }

    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            emitter.comment("spread array as indirect variadic param");
            let ty = super::super::emit_expr(spread_expr, emitter, ctx, data);
            super::super::retain_borrowed_heap_arg(emitter, spread_expr, &ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // push variadic array pointer onto stack
            arg_types.push(ty);
        } else if variadic_args.is_empty() {
            emitter.comment("empty indirect variadic array");
            emitter.instruction("mov x0, #4");                                  // initial capacity: 4 (grows dynamically)
            emitter.instruction("mov x1, #8");                                  // element size: 8 bytes
            emitter.instruction("bl __rt_array_new");                           // allocate empty array for variadic param
            emitter.instruction("str x0, [sp, #-16]!");                         // push empty variadic array onto stack
            arg_types.push(PhpType::Array(Box::new(PhpType::Int)));
        } else {
            let n = variadic_args.len();
            emitter.comment(&format!("build indirect variadic array ({} elements)", n));
            let first_elem_ty = functions::infer_contextual_type(variadic_args[0], ctx);
            let es: usize = match &first_elem_ty {
                PhpType::Str => 16,
                _ => 8,
            };
            emitter.instruction(&format!("mov x0, #{}", n));                    // capacity: exact element count
            emitter.instruction(&format!("mov x1, #{}", es));                   // element size in bytes
            emitter.instruction("bl __rt_array_new");                           // allocate array for variadic args
            emitter.instruction("str x0, [sp, #-16]!");                         // save variadic array pointer on stack

            for (i, varg) in variadic_args.iter().enumerate() {
                let ty = super::super::emit_expr(varg, emitter, ctx, data);
                super::super::retain_borrowed_heap_arg(emitter, varg, &ty);
                emitter.instruction("ldr x9, [sp]");                            // peek variadic array pointer from stack
                if i == 0 {
                    super::super::arrays::emit_array_value_type_stamp(emitter, "x9", &ty);
                }
                match &ty {
                    PhpType::Int | PhpType::Bool | PhpType::Callable => {
                        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); //store int-like variadic element
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("str d0, [x9, #{}]", 24 + i * 8)); //store float variadic element
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("str x1, [x9, #{}]", 24 + i * 16)); //store variadic string pointer
                        emitter.instruction(&format!("str x2, [x9, #{}]", 24 + i * 16 + 8)); //store variadic string length
                    }
                    PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); //store refcounted variadic payload
                    }
                    _ => {}
                }
                emitter.instruction(&format!("mov x10, #{}", i + 1));           // new variadic array length after this element
                emitter.instruction("str x10, [x9]");                           // persist updated variadic array length
            }

            arg_types.push(PhpType::Array(Box::new(first_elem_ty)));
        }
    }

    let _callee_ty = super::super::emit_expr(callee, emitter, ctx, data);
    emitter.instruction("mov x9, x0");                                          // save closure address to x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    let assignments = args::build_arg_assignments(&arg_types, 0);

    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9
    let overflow_bytes = args::materialize_call_args(emitter, &assignments, arg_types.len());

    let ret_ty = callee_sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or_else(|| match &callee.kind {
            ExprKind::Closure { body, .. } => crate::types::checker::infer_return_type_syntactic(body),
            _ => PhpType::Int,
        });

    emitter.instruction("mov x19, x9");                                         // preserve closure address across concat-offset save
    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // branch to closure via function pointer in x19
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack arguments after the indirect call returns
    }

    ret_ty
}
