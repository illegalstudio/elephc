use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Platform;
use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::context::{Context, TRY_HANDLER_SLOT_SIZE};

pub(super) fn collect_constants(
    program: &Program,
    target_platform: Platform,
) -> HashMap<String, (ExprKind, PhpType)> {
    let mut constants = HashMap::new();
    constants.insert(
        "PHP_OS".to_string(),
        (
            ExprKind::StringLiteral(target_platform.php_os_name().to_string()),
            PhpType::Str,
        ),
    );
    for stmt in program {
        match &stmt.kind {
            StmtKind::ConstDecl { name, value } => {
                let ty = match &value.kind {
                    ExprKind::IntLiteral(_) => PhpType::Int,
                    ExprKind::FloatLiteral(_) => PhpType::Float,
                    ExprKind::StringLiteral(_) => PhpType::Str,
                    ExprKind::BoolLiteral(_) => PhpType::Bool,
                    _ => PhpType::Int,
                };
                constants.entry(name.clone()).or_insert((value.kind.clone(), ty));
            }
            StmtKind::ExprStmt(expr) => {
                if let ExprKind::FunctionCall { name, args } = &expr.kind {
                    if name.as_str() == "define" && args.len() == 2 {
                        if let ExprKind::StringLiteral(const_name) = &args[0].kind {
                            let ty = match &args[1].kind {
                                ExprKind::IntLiteral(_) => PhpType::Int,
                                ExprKind::FloatLiteral(_) => PhpType::Float,
                                ExprKind::StringLiteral(_) => PhpType::Str,
                                ExprKind::BoolLiteral(_) => PhpType::Bool,
                                _ => PhpType::Int,
                            };
                            constants
                                .entry(const_name.clone())
                                .or_insert((args[1].kind.clone(), ty));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    constants
}

pub(super) fn collect_global_var_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in program {
        if let StmtKind::FunctionDecl { body, .. } = &stmt.kind {
            collect_global_vars_in_body(body, &mut names);
        }
    }
    names
}

fn collect_global_vars_in_body(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Global { vars } => {
                for v in vars {
                    names.insert(v.clone());
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_global_vars_in_body(then_body, names);
                for (_, body) in elseif_clauses {
                    collect_global_vars_in_body(body, names);
                }
                if let Some(body) = else_body {
                    collect_global_vars_in_body(body, names);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => collect_global_vars_in_body(body, names),
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_global_vars_in_body(try_body, names);
                for catch_clause in catches {
                    collect_global_vars_in_body(&catch_clause.body, names);
                }
                if let Some(body) = finally_body {
                    collect_global_vars_in_body(body, names);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_global_vars_in_body(body, names);
                }
                if let Some(body) = default {
                    collect_global_vars_in_body(body, names);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn collect_static_vars(
    program: &Program,
    global_env: &TypeEnv,
) -> HashMap<(String, String), PhpType> {
    let mut statics = HashMap::new();
    for stmt in program {
        if let StmtKind::FunctionDecl { name, body, .. } = &stmt.kind {
            collect_static_vars_in_body(name, body, &mut statics, global_env);
        }
    }
    statics
}

fn collect_static_vars_in_body(
    func_name: &str,
    stmts: &[Stmt],
    statics: &mut HashMap<(String, String), PhpType>,
    global_env: &TypeEnv,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::StaticVar { name, init } => {
                let ty = match &init.kind {
                    ExprKind::IntLiteral(_) => PhpType::Int,
                    ExprKind::FloatLiteral(_) => PhpType::Float,
                    ExprKind::StringLiteral(_) => PhpType::Str,
                    ExprKind::BoolLiteral(_) => PhpType::Bool,
                    _ => PhpType::Int,
                };
                statics.insert((func_name.to_string(), name.clone()), ty);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_static_vars_in_body(func_name, then_body, statics, global_env);
                for (_, body) in elseif_clauses {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
                if let Some(body) = else_body {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_static_vars_in_body(func_name, body, statics, global_env);
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_static_vars_in_body(func_name, try_body, statics, global_env);
                for catch_clause in catches {
                    collect_static_vars_in_body(
                        func_name,
                        &catch_clause.body,
                        statics,
                        global_env,
                    );
                }
                if let Some(body) = finally_body {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
                if let Some(body) = default {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
            }
            _ => {}
        }
    }
    let _ = global_env;
}

pub(super) fn collect_main_try_slots(stmts: &[Stmt], ctx: &mut Context) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let slot_offset = ctx.alloc_hidden_slot(TRY_HANDLER_SLOT_SIZE);
                ctx.try_slot_offsets.push(slot_offset);
                collect_main_try_slots(try_body, ctx);
                for catch_clause in catches {
                    collect_main_try_slots(&catch_clause.body, ctx);
                }
                if let Some(body) = finally_body {
                    collect_main_try_slots(body, ctx);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_main_try_slots(then_body, ctx);
                for (_, body) in elseif_clauses {
                    collect_main_try_slots(body, ctx);
                }
                if let Some(body) = else_body {
                    collect_main_try_slots(body, ctx);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => collect_main_try_slots(body, ctx),
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(s) = init {
                    collect_main_try_slots(&[*s.clone()], ctx);
                }
                if let Some(s) = update {
                    collect_main_try_slots(&[*s.clone()], ctx);
                }
                collect_main_try_slots(body, ctx);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_main_try_slots(body, ctx);
                }
                if let Some(body) = default {
                    collect_main_try_slots(body, ctx);
                }
            }
            StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => {}
            _ => {}
        }
    }
}
