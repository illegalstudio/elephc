use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

pub(super) fn emit_function_call(
    name: &str,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call {}()", name));

    let sig = ctx.functions.get(name).cloned();
    let is_variadic = sig.as_ref().map(|s| s.variadic.is_some()).unwrap_or(false);

    let regular_param_count = if is_variadic {
        sig.as_ref()
            .map(|s| s.params.len().saturating_sub(1))
            .unwrap_or(0)
    } else {
        sig.as_ref().map(|s| s.params.len()).unwrap_or(args_exprs.len())
    };

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
        if let Some(ref s) = sig {
            for i in all_args.len()..regular_param_count {
                if let Some(Some(default)) = s.defaults.get(i) {
                    default_exprs.push(default.clone());
                }
            }
        }
        let default_refs: Vec<&Expr> = default_exprs.iter().collect();
        all_args.extend(default_refs);
    }

    let ref_params = sig
        .as_ref()
        .map(|s| s.ref_params.clone())
        .unwrap_or_default();

    let mut arg_types = Vec::new();
    for (i, arg) in all_args.iter().enumerate() {
        let is_ref = ref_params.get(i).copied().unwrap_or(false);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if ctx.global_vars.contains(var_name) {
                    let label = format!("_gvar_{}", var_name);
                    emitter.comment(&format!("ref arg: address of global ${}", var_name));
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));       // load page of global var
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); // add page offset
                } else {
                    let var = match ctx.variables.get(var_name) {
                        Some(v) => v,
                        None => {
                            emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                            continue;
                        }
                    };
                    let offset = var.stack_offset;
                    emitter.comment(&format!("ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", offset));      // compute address of local variable
                }
            } else {
                super::super::emit_expr(arg, emitter, ctx, data);
            }
            emitter.instruction("str x0, [sp, #-16]!");                             // push address onto stack
            arg_types.push(PhpType::Int);
        } else {
            let ty = super::super::emit_expr(arg, emitter, ctx, data);
            super::super::retain_borrowed_heap_arg(emitter, arg, &ty);
            args::push_arg_value(emitter, &ty);
            arg_types.push(ty);
        }
    }

    if spread_into_named {
        if let Some(spread_expr) = spread_arg {
            let remaining = regular_param_count - spread_at_index;
            emitter.comment(&format!("unpack spread into {} named params", remaining));
            let _ty = super::super::emit_expr(spread_expr, emitter, ctx, data);

            let elem_ty = if let Some(ref s) = sig {
                if spread_at_index < s.params.len() {
                    s.params[spread_at_index].1.clone()
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            };

            emitter.instruction("mov x9, x0");                                      // save array pointer in x9
            emitter.instruction("add x9, x9, #24");                                 // skip 24-byte array header to reach data
            for idx in 0..remaining {
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", idx * 8)); // load int element at offset index*8
                        emitter.instruction("str x0, [sp, #-16]!");                 // push unpacked int arg onto stack
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("ldr d0, [x9, #{}]", idx * 8)); // load float element at offset index*8
                        emitter.instruction("str d0, [sp, #-16]!");                 // push unpacked float arg onto stack
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("ldr x1, [x9, #{}]", idx * 16)); // load string pointer at offset index*16
                        emitter.instruction(&format!("ldr x2, [x9, #{}]", idx * 16 + 8)); // load string length at offset index*16+8
                        emitter.instruction("stp x1, x2, [sp, #-16]!");             // push unpacked string arg onto stack
                    }
                    _ => {
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", idx * 8)); // load element at offset index*8
                        emitter.instruction("str x0, [sp, #-16]!");                 // push unpacked arg onto stack
                    }
                }
                arg_types.push(elem_ty.clone());
            }
        }
    }

    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            emitter.comment("spread array as variadic param");
            let ty = super::super::emit_expr(spread_expr, emitter, ctx, data);
            super::super::retain_borrowed_heap_arg(emitter, spread_expr, &ty);
            emitter.instruction("str x0, [sp, #-16]!");                             // push variadic array pointer onto stack
        } else if variadic_args.is_empty() {
            emitter.comment("empty variadic array");
            emitter.instruction("mov x0, #4");                                      // initial capacity: 4 (grows dynamically)
            emitter.instruction("mov x1, #8");                                      // element size: 8 bytes
            emitter.instruction("bl __rt_array_new");                               // allocate empty array for variadic param
            emitter.instruction("str x0, [sp, #-16]!");                             // push empty variadic array onto stack
        } else {
            let n = variadic_args.len();
            emitter.comment(&format!("build variadic array ({} elements)", n));
            let first_elem_ty = match &variadic_args[0].kind {
                ExprKind::StringLiteral(_) => PhpType::Str,
                _ => PhpType::Int,
            };
            let es: usize = match &first_elem_ty {
                PhpType::Str => 16,
                _ => 8,
            };
            emitter.instruction(&format!("mov x0, #{}", n));                        // capacity: exact element count (grows if needed)
            emitter.instruction(&format!("mov x1, #{}", es));                       // element size in bytes
            emitter.instruction("bl __rt_array_new");                               // allocate array for variadic args
            emitter.instruction("str x0, [sp, #-16]!");                             // save variadic array pointer on stack

            for (i, varg) in variadic_args.iter().enumerate() {
                let ty = super::super::emit_expr(varg, emitter, ctx, data);
                emitter.instruction("ldr x9, [sp]");                                // peek variadic array pointer from stack
                match &ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); // store int element at data offset
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("str d0, [x9, #{}]", 24 + i * 8)); // store float element at data offset
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("str x1, [x9, #{}]", 24 + i * 16)); // store string pointer at data offset
                        emitter.instruction(&format!("str x2, [x9, #{}]", 24 + i * 16 + 8)); // store string length right after pointer
                    }
                    PhpType::Array(_) | PhpType::AssocArray { .. } => {
                        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); // store nested array pointer at data offset
                    }
                    _ => {}
                }
                emitter.instruction(&format!("mov x10, #{}", i + 1));               // new length after adding this element
                emitter.instruction("str x10, [x9]");                               // write updated length to array header
            }
        }
        arg_types.push(PhpType::Array(Box::new(PhpType::Int)));
    }

    let assignments = args::build_arg_assignments(&arg_types, 0);
    args::load_arg_assignments(emitter, &assignments, arg_types.len());

    let ret_ty = ctx
        .functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void);

    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction(&format!("bl _fn_{}", name));                               // branch-and-link to compiled PHP function
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}
