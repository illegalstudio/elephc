use std::collections::HashMap;

use crate::codegen::context::{Context, HeapOwnership, TRY_HANDLER_SLOT_SIZE};
use crate::parser::ast::StmtKind;
use crate::types::{FunctionSig, PhpType};

use super::types::infer_local_type;

pub(super) fn mark_control_flow_epilogue_unsafe(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
    in_control_flow: bool,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, .. } => {
                if in_control_flow {
                    ctx.disable_epilogue_cleanup(name);
                }
            }
            StmtKind::ListUnpack { vars, .. } => {
                if in_control_flow {
                    for var in vars {
                        ctx.disable_epilogue_cleanup(var);
                    }
                }
            }
            StmtKind::Global { vars } => {
                for var in vars {
                    ctx.disable_epilogue_cleanup(var);
                }
            }
            StmtKind::StaticVar { name, .. } => {
                ctx.disable_epilogue_cleanup(name);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                let direct_assigns = exhaustive_if_direct_heap_assignments(
                    then_body,
                    elseif_clauses,
                    else_body,
                    ctx,
                    sig,
                );
                mark_control_flow_epilogue_unsafe(then_body, ctx, sig, true);
                for (_, body) in elseif_clauses {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
                if let Some(body) = else_body {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
                for (name, ty) in direct_assigns {
                    if ctx.global_vars.contains(&name)
                        || ctx.static_vars.contains(&name)
                        || ctx.ref_params.contains(&name)
                    {
                        continue;
                    }
                    let Some(var) = ctx.variables.get(&name) else {
                        continue;
                    };
                    if var.ty != ty {
                        continue;
                    }
                    ctx.update_var_type_and_ownership(
                        &name,
                        ty.clone(),
                        HeapOwnership::local_owner_for_type(&ty),
                    );
                    ctx.enable_epilogue_cleanup(&name);
                }
            }
            StmtKind::Foreach {
                body,
                key_var,
                value_var,
                ..
            } => {
                ctx.disable_epilogue_cleanup(value_var);
                if let Some(key_var) = key_var {
                    ctx.disable_epilogue_cleanup(key_var);
                }
                mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
            }
            StmtKind::DoWhile { body, .. } | StmtKind::While { body, .. } => {
                mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
            }
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(stmt) = init {
                    mark_control_flow_epilogue_unsafe(
                        std::slice::from_ref(stmt.as_ref()),
                        ctx,
                        sig,
                        true,
                    );
                }
                if let Some(stmt) = update {
                    mark_control_flow_epilogue_unsafe(
                        std::slice::from_ref(stmt.as_ref()),
                        ctx,
                        sig,
                        true,
                    );
                }
                mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
                if let Some(body) = default {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                mark_control_flow_epilogue_unsafe(try_body, ctx, sig, true);
                for catch_clause in catches {
                    mark_control_flow_epilogue_unsafe(&catch_clause.body, ctx, sig, true);
                }
                if let Some(body) = finally_body {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn collect_try_slots(stmts: &[crate::parser::ast::Stmt], ctx: &mut Context) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let slot_offset = ctx.alloc_hidden_slot(TRY_HANDLER_SLOT_SIZE);
                ctx.try_slot_offsets.push(slot_offset);
                collect_try_slots(try_body, ctx);
                for catch_clause in catches {
                    collect_try_slots(&catch_clause.body, ctx);
                }
                if let Some(body) = finally_body {
                    collect_try_slots(body, ctx);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_try_slots(then_body, ctx);
                for (_, body) in elseif_clauses {
                    collect_try_slots(body, ctx);
                }
                if let Some(body) = else_body {
                    collect_try_slots(body, ctx);
                }
            }
            StmtKind::Foreach { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. } => collect_try_slots(body, ctx),
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(s) = init {
                    collect_try_slots(&[*s.clone()], ctx);
                }
                if let Some(s) = update {
                    collect_try_slots(&[*s.clone()], ctx);
                }
                collect_try_slots(body, ctx);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_try_slots(body, ctx);
                }
                if let Some(body) = default {
                    collect_try_slots(body, ctx);
                }
            }
            _ => {}
        }
    }
}

fn collect_straight_line_direct_assignments(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &Context,
    sig: &FunctionSig,
) -> (HashMap<String, PhpType>, bool) {
    let mut assignments = HashMap::new();
    let mut may_fall_through = true;

    for stmt in stmts {
        if !may_fall_through {
            break;
        }
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                assignments.insert(name.clone(), infer_local_type(value, sig, Some(ctx)));
            }
            StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => {
                may_fall_through = false;
            }
            _ => {}
        }
    }

    (assignments, may_fall_through)
}

fn exhaustive_if_direct_heap_assignments(
    then_body: &[crate::parser::ast::Stmt],
    elseif_clauses: &[(crate::parser::ast::Expr, Vec<crate::parser::ast::Stmt>)],
    else_body: &Option<Vec<crate::parser::ast::Stmt>>,
    ctx: &Context,
    sig: &FunctionSig,
) -> HashMap<String, PhpType> {
    let Some(else_body) = else_body.as_ref() else {
        return HashMap::new();
    };

    let mut branch_assignments = Vec::new();
    let (then_assigns, then_falls_through) =
        collect_straight_line_direct_assignments(then_body, ctx, sig);
    if then_falls_through {
        branch_assignments.push(then_assigns);
    }
    for (_, body) in elseif_clauses {
        let (assigns, falls_through) = collect_straight_line_direct_assignments(body, ctx, sig);
        if falls_through {
            branch_assignments.push(assigns);
        }
    }
    let (else_assigns, else_falls_through) =
        collect_straight_line_direct_assignments(else_body, ctx, sig);
    if else_falls_through {
        branch_assignments.push(else_assigns);
    }

    let Some((first_branch, remaining_branches)) = branch_assignments.split_first() else {
        return HashMap::new();
    };
    let mut definitely_assigned = first_branch.clone();
    definitely_assigned.retain(|name, ty| {
        (matches!(ty, PhpType::Str) || ty.is_refcounted())
            && remaining_branches
                .iter()
                .all(|assigns| assigns.get(name) == Some(ty))
    });
    definitely_assigned
}
