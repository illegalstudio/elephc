use crate::codegen::context::{Context, DeferredClosure};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType};

use super::args;

fn infer_closure_return_type(body: &[Stmt], sig: &FunctionSig) -> PhpType {
    fn collect_return_types(stmt: &Stmt, sig: &FunctionSig, return_types: &mut Vec<PhpType>) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                return_types.push(crate::codegen::functions::infer_local_type_pub(expr, sig));
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
        result = super::super::widen_codegen_type(&result, ty);
    }
    result
}

pub(super) fn emit_closure(
    params: &[(String, Option<Expr>, bool)],
    body: &[Stmt],
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

    let mut param_types: Vec<(String, PhpType)> =
        params.iter().map(|(p, _, _)| (p.clone(), PhpType::Int)).collect();
    for (cap_name, cap_ty) in &capture_types {
        param_types.push((cap_name.clone(), cap_ty.clone()));
    }
    let mut defaults: Vec<Option<Expr>> =
        params.iter().map(|(_, default, _)| default.clone()).collect();
    for _ in &capture_types {
        defaults.push(None);
    }
    let mut ref_params: Vec<bool> = params.iter().map(|(_, _, is_ref)| *is_ref).collect();
    ref_params.extend(std::iter::repeat_n(false, capture_types.len()));
    let preliminary_sig = FunctionSig {
        params: param_types.clone(),
        defaults: defaults.clone(),
        return_type: PhpType::Int,
        ref_params: ref_params.clone(),
        variadic: None,
    };
    let return_type = infer_closure_return_type(body, &preliminary_sig);
    let sig = FunctionSig {
        params: param_types,
        defaults,
        return_type,
        ref_params,
        variadic: None,
    };

    let param_names: Vec<String> = params.iter().map(|(n, _, _)| n.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: closure_label.clone(),
        params: param_names,
        body: body.to_vec(),
        sig,
        captures: capture_types,
    });

    emitter.comment("closure: load function address");
    emitter.instruction(&format!("adrp x0, {}@PAGE", closure_label));               // load page base of closure function
    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", closure_label));         // add page offset to get exact closure address
    PhpType::Callable
}

pub(super) fn emit_closure_call(
    var: &str,
    args_exprs: &[Expr],
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
        .unwrap_or(args_exprs.len());

    let mut all_args: Vec<&Expr> = args_exprs.iter().collect();
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
        let ty = super::super::emit_expr(arg, emitter, ctx, data);
        args::push_arg_value(emitter, &ty);
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
                crate::codegen::abi::load_at_offset(emitter, "x0", cap_offset);     // load captured int/bool/array value
                emitter.instruction("str x0, [sp, #-16]!");                         // push captured value onto stack
            }
            PhpType::Float => {
                crate::codegen::abi::load_at_offset(emitter, "d0", cap_offset);     // load captured float value
                emitter.instruction("str d0, [sp, #-16]!");                         // push captured float onto stack
            }
            PhpType::Str => {
                crate::codegen::abi::load_at_offset(emitter, "x1", cap_offset);     // load captured string pointer
                crate::codegen::abi::load_at_offset(emitter, "x2", cap_offset - 8); // load captured string length
                emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push captured string ptr+len onto stack
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
    crate::codegen::abi::load_at_offset(emitter, "x9", var_offset);                // load closure function address from stack
    emitter.instruction("str x9, [sp, #-16]!");                                     // push closure address temporarily

    let assignments = args::build_arg_assignments(&arg_types, 0);

    emitter.instruction("ldr x9, [sp], #16");                                       // pop closure function address into x9
    args::load_arg_assignments(emitter, &assignments, total_args);

    let ret_ty = ctx
        .closure_sigs
        .get(var)
        .map(|s| s.return_type.clone())
        .unwrap_or(PhpType::Int);

    emitter.instruction("mov x19, x9");                                             // preserve closure address across concat-offset save
    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                                 // branch to closure via function pointer in x19
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}
