use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{
    emit_expr, restore_concat_offset_after_nested_call, retain_borrowed_heap_arg,
    save_concat_offset_before_nested_call, widen_codegen_type, Expr, ExprKind, PhpType,
};

pub(super) fn emit_function_call(
    name: &str,
    args: &[Expr],
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
        sig.as_ref().map(|s| s.params.len()).unwrap_or(args.len())
    };

    let mut regular_args: Vec<&Expr> = Vec::new();
    let mut variadic_args: Vec<&Expr> = Vec::new();
    let mut spread_arg: Option<&Expr> = None;
    let mut spread_at_index: usize = 0;

    for (i, arg) in args.iter().enumerate() {
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
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));   // load page of global var
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); //add page offset
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
                    emitter.instruction(&format!("sub x0, x29, #{}", offset));  // compute address of local variable
                }
            } else {
                emit_expr(arg, emitter, ctx, data);
            }
            emitter.instruction("str x0, [sp, #-16]!");                         // push address onto stack
            arg_types.push(PhpType::Int);
        } else {
            let ty = emit_expr(arg, emitter, ctx, data);
            retain_borrowed_heap_arg(emitter, arg, &ty);
            match &ty {
                PhpType::Bool
                | PhpType::Int
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Pointer(_) => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push int/bool/array-ptr/callable arg onto stack
                }
                PhpType::Float => {
                    emitter.instruction("str d0, [sp, #-16]!");                 // push float arg onto stack
                }
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // push string ptr+len arg onto stack
                }
                PhpType::Void => {}
            }
            arg_types.push(ty);
        }
    }

    if spread_into_named {
        if let Some(spread_expr) = spread_arg {
            let remaining = regular_param_count - spread_at_index;
            emitter.comment(&format!("unpack spread into {} named params", remaining));
            let _ty = emit_expr(spread_expr, emitter, ctx, data);

            let elem_ty = if let Some(ref s) = sig {
                if spread_at_index < s.params.len() {
                    s.params[spread_at_index].1.clone()
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            };

            emitter.instruction("mov x9, x0");                                  // save array pointer in x9
            emitter.instruction("add x9, x9, #24");                             // skip 24-byte array header to reach data
            for idx in 0..remaining {
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", idx * 8)); // load int element at offset index*8
                        emitter.instruction("str x0, [sp, #-16]!");             // push unpacked int arg onto stack
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("ldr d0, [x9, #{}]", idx * 8)); // load float element at offset index*8
                        emitter.instruction("str d0, [sp, #-16]!");             // push unpacked float arg onto stack
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("ldr x1, [x9, #{}]", idx * 16)); // load string pointer at offset index*16
                        emitter.instruction(&format!("ldr x2, [x9, #{}]", idx * 16 + 8)); // load string length at offset index*16+8
                        emitter.instruction("stp x1, x2, [sp, #-16]!");         // push unpacked string arg onto stack
                    }
                    _ => {
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", idx * 8)); // load element at offset index*8
                        emitter.instruction("str x0, [sp, #-16]!");             // push unpacked arg onto stack
                    }
                }
                arg_types.push(elem_ty.clone());
            }
        }
    }

    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            emitter.comment("spread array as variadic param");
            let ty = emit_expr(spread_expr, emitter, ctx, data);
            retain_borrowed_heap_arg(emitter, spread_expr, &ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // push variadic array pointer onto stack
        } else if variadic_args.is_empty() {
            emitter.comment("empty variadic array");
            emitter.instruction("mov x0, #4");                                  // initial capacity: 4 (grows dynamically)
            emitter.instruction("mov x1, #8");                                  // element size: 8 bytes
            emitter.instruction("bl __rt_array_new");                           // allocate empty array for variadic param
            emitter.instruction("str x0, [sp, #-16]!");                         // push empty variadic array onto stack
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
            emitter.instruction(&format!("mov x0, #{}", n));                    // capacity: exact element count (grows if needed)
            emitter.instruction(&format!("mov x1, #{}", es));                   // element size in bytes
            emitter.instruction("bl __rt_array_new");                           // allocate array for variadic args
            emitter.instruction("str x0, [sp, #-16]!");                         // save variadic array pointer on stack

            for (i, varg) in variadic_args.iter().enumerate() {
                let ty = emit_expr(varg, emitter, ctx, data);
                emitter.instruction("ldr x9, [sp]");                            // peek variadic array pointer from stack
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
                emitter.instruction(&format!("mov x10, #{}", i + 1));           // new length after adding this element
                emitter.instruction("str x10, [x9]");                           // write updated length to array header
            }
        }
        arg_types.push(PhpType::Array(Box::new(PhpType::Int)));
    }

    let total_args = arg_types.len();
    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

    for i in (0..total_args).rev() {
        let (ty, start_reg, is_float) = &assignments[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop int/bool/array/callable arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg into float register
            }
            PhpType::Str => {
                emitter.instruction(&format!(
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
        let _ = is_float;
    }

    let ret_ty = ctx
        .functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void);

    save_concat_offset_before_nested_call(emitter);
    emitter.instruction(&format!("bl _fn_{}", name));                           // branch-and-link to compiled PHP function
    restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}

fn infer_closure_return_type(
    body: &[crate::parser::ast::Stmt],
    sig: &crate::types::FunctionSig,
) -> PhpType {
    fn collect_return_types(
        stmt: &crate::parser::ast::Stmt,
        sig: &crate::types::FunctionSig,
        return_types: &mut Vec<PhpType>,
    ) {
        use crate::parser::ast::StmtKind;

        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                return_types.push(super::super::functions::infer_local_type_pub(expr, sig));
            }
            StmtKind::Return(None) => {
                return_types.push(PhpType::Void);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for stmt in then_body {
                    collect_return_types(stmt, sig, return_types);
                }
                for (_, body) in elseif_clauses {
                    for stmt in body {
                        collect_return_types(stmt, sig, return_types);
                    }
                }
                if let Some(body) = else_body {
                    for stmt in body {
                        collect_return_types(stmt, sig, return_types);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for stmt in body {
                    collect_return_types(stmt, sig, return_types);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for stmt in body {
                        collect_return_types(stmt, sig, return_types);
                    }
                }
                if let Some(body) = default {
                    for stmt in body {
                        collect_return_types(stmt, sig, return_types);
                    }
                }
            }
            _ => {}
        }
    }

    let mut return_types = Vec::new();
    for stmt in body {
        collect_return_types(stmt, sig, &mut return_types);
    }
    if return_types.is_empty() {
        return PhpType::Int;
    }
    let mut result = return_types[0].clone();
    for ty in &return_types[1..] {
        result = widen_codegen_type(&result, ty);
    }
    result
}

pub(super) fn emit_closure(
    params: &[(String, Option<Expr>, bool)],
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> PhpType {
    let closure_label = ctx.next_label("closure");

    let mut capture_types: Vec<(String, PhpType)> = Vec::new();
    for cap_name in captures {
        let ty = ctx
            .variables
            .get(cap_name)
            .map(|v| v.ty.clone())
            .unwrap_or(PhpType::Int);
        capture_types.push((cap_name.clone(), ty));
    }

    let mut param_types: Vec<(String, crate::types::PhpType)> =
        params.iter().map(|(p, _, _)| (p.clone(), PhpType::Int)).collect();
    for (cap_name, cap_ty) in &capture_types {
        param_types.push((cap_name.clone(), cap_ty.clone()));
    }
    let mut defaults: Vec<Option<crate::parser::ast::Expr>> =
        params.iter().map(|(_, default, _)| default.clone()).collect();
    for _ in &capture_types {
        defaults.push(None);
    }
    let mut ref_params: Vec<bool> = params.iter().map(|(_, _, is_ref)| *is_ref).collect();
    ref_params.extend(std::iter::repeat_n(false, capture_types.len()));
    let preliminary_sig = crate::types::FunctionSig {
        params: param_types.clone(),
        defaults: defaults.clone(),
        return_type: PhpType::Int,
        ref_params: ref_params.clone(),
        variadic: None,
    };
    let return_type = infer_closure_return_type(body, &preliminary_sig);
    let sig = crate::types::FunctionSig {
        params: param_types,
        defaults,
        return_type,
        ref_params,
        variadic: None,
    };

    let param_names: Vec<String> = params.iter().map(|(n, _, _)| n.clone()).collect();
    ctx.deferred_closures.push(super::super::context::DeferredClosure {
        label: closure_label.clone(),
        params: param_names,
        body: body.to_vec(),
        sig,
        captures: capture_types,
    });

    emitter.comment("closure: load function address");
    emitter.instruction(&format!("adrp x0, {}@PAGE", closure_label));           // load page base of closure function
    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", closure_label));     // add page offset to get exact closure address
    PhpType::Callable
}

pub(super) fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call ${}()", var));

    let sig = ctx.closure_sigs.get(var).cloned();
    let captures = ctx.closure_captures.get(var).cloned().unwrap_or_default();
    let visible_param_count = sig
        .as_ref()
        .map(|s| s.params.len() - captures.len())
        .unwrap_or(args.len());

    let mut all_args: Vec<&Expr> = args.iter().collect();
    let mut default_exprs: Vec<Expr> = Vec::new();
    if let Some(ref s) = sig {
        for i in all_args.len()..visible_param_count {
            if let Some(Some(default)) = s.defaults.get(i) {
                default_exprs.push(default.clone());
            }
        }
    }
    let default_refs: Vec<&Expr> = default_exprs.iter().collect();
    all_args.extend(default_refs);

    let mut arg_types = Vec::new();
    for arg in &all_args {
        let ty = emit_expr(arg, emitter, ctx, data);
        match &ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int/bool/array/callable arg onto stack
            }
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push float arg onto stack
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len arg onto stack
            }
            PhpType::Void => {}
        }
        arg_types.push(ty);
    }

    if let Some(cached_sig) = ctx.closure_sigs.get(var).cloned() {
        for deferred in &mut ctx.deferred_closures {
            if deferred.sig.params == cached_sig.params {
                for (i, ty) in arg_types.iter().enumerate() {
                    if i < deferred.sig.params.len() {
                        deferred.sig.params[i].1 = ty.clone();
                    }
                }
                break;
            }
        }
        if let Some(cached) = ctx.closure_sigs.get_mut(var) {
            for (i, ty) in arg_types.iter().enumerate() {
                if i < cached.params.len() {
                    cached.params[i].1 = ty.clone();
                }
            }
        }
    }

    for (cap_name, cap_ty) in &captures {
        emitter.comment(&format!("push captured ${}", cap_name));
        let cap_info = match ctx.variables.get(cap_name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: captured variable ${} not found", cap_name));
                continue;
            }
        };
        let cap_offset = cap_info.stack_offset;
        match cap_ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                abi::load_at_offset(emitter, "x0", cap_offset);                 // load captured int/bool/array value
                emitter.instruction("str x0, [sp, #-16]!");                     // push captured value onto stack
            }
            PhpType::Float => {
                abi::load_at_offset(emitter, "d0", cap_offset);                 // load captured float value
                emitter.instruction("str d0, [sp, #-16]!");                     // push captured float onto stack
            }
            PhpType::Str => {
                abi::load_at_offset(emitter, "x1", cap_offset);                 // load captured string pointer
                abi::load_at_offset(emitter, "x2", cap_offset - 8);             // load captured string length
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push captured string ptr+len onto stack
            }
            PhpType::Void => {}
        }
        arg_types.push(cap_ty.clone());
    }
    let total_args = all_args.len() + captures.len();

    let var_info = match ctx.variables.get(var) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined closure variable ${}", var));
            return PhpType::Int;
        }
    };
    let var_offset = var_info.stack_offset;
    abi::load_at_offset(emitter, "x9", var_offset);                            // load closure function address from stack
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9

    for i in (0..total_args).rev() {
        let (ty, start_reg, _is_float) = &assignments[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop int/bool/array/callable arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg into float register
            }
            PhpType::Str => {
                emitter.instruction(&format!(
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }

    let ret_ty = ctx
        .closure_sigs
        .get(var)
        .map(|s| s.return_type.clone())
        .unwrap_or(PhpType::Int);

    emitter.instruction("mov x19, x9");                                         // preserve closure address across concat-offset save
    save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // branch to closure via function pointer in x19
    restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}

pub(super) fn emit_expr_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call expression result");

    let mut arg_types = Vec::new();
    for arg in args {
        let ty = emit_expr(arg, emitter, ctx, data);
        retain_borrowed_heap_arg(emitter, arg, &ty);
        match &ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int/bool/array/callable arg onto stack
            }
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push float arg onto stack
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len arg onto stack
            }
            PhpType::Void => {}
        }
        arg_types.push(ty);
    }

    let _callee_ty = emit_expr(callee, emitter, ctx, data);
    emitter.instruction("mov x9, x0");                                          // save closure address to x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9

    for i in (0..args.len()).rev() {
        let (ty, start_reg, _is_float) = &assignments[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop int/bool/array/callable arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg into float register
            }
            PhpType::Str => {
                emitter.instruction(&format!(
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }

    let ret_ty = match &callee.kind {
        ExprKind::Variable(var_name) => {
            if let Some(sig) = ctx.closure_sigs.get(var_name) {
                sig.return_type.clone()
            } else {
                PhpType::Int
            }
        }
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(arr_name) = &array.kind {
                if let Some(sig) = ctx.closure_sigs.get(arr_name) {
                    sig.return_type.clone()
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            }
        }
        ExprKind::Closure { body, .. } => crate::types::checker::infer_return_type_syntactic(body),
        _ => PhpType::Int,
    };

    emitter.instruction("mov x19, x9");                                         // preserve closure address across concat-offset save
    save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // branch to closure via function pointer in x19
    restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}
