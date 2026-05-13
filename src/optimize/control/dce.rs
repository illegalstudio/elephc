//! Purpose:
//! Eliminates unreachable or redundant statements after effect and control-flow analysis.
//! Coordinates write tracking, branch guards, switch/try handling, method effects, and tail sinking.
//!
//! Called from:
//! - `crate::optimize::eliminate_dead_code()`
//!
//! Key details:
//! - DCE may drop code only when effects, writes, terminal control flow, and guard exclusions prove it unobservable.

use super::*;

mod guards;
mod ifs;
mod methods;
mod state;
mod switches;
mod tail;
mod tries;
mod writes;

pub(crate) use methods::{dce_method, dce_method_without_context};
use guards::*;
use ifs::dce_if_stmt;
use state::{GuardLiteral, GuardState};
use switches::direct_switch_entry_blocks;
use switches::dce_switch_stmt;
use tail::dce_stmt_with_tail;
use tries::dce_try_stmt;
use writes::*;

pub(crate) fn dce_block(body: Vec<Stmt>) -> Vec<Stmt> {
    dce_block_with_guards(body, GuardState::default())
}

fn dce_block_with_guards(body: Vec<Stmt>, mut guards: GuardState) -> Vec<Stmt> {
    let mut eliminated = Vec::new();
    let mut stmts = body.into_iter().peekable();
    while let Some(stmt) = stmts.next() {
        let has_tail = stmts.peek().is_some();
        let use_tail_sink = has_tail
            && matches!(
                stmt.kind,
                StmtKind::If { .. } | StmtKind::IfDef { .. } | StmtKind::Switch { .. } | StmtKind::Try { .. }
            );
        let dce_stmt = if use_tail_sink {
            let tail: Vec<Stmt> = stmts.clone().collect();
            dce_stmt_with_tail(stmt, tail, &guards)
        } else {
            dce_stmt_with_guards(stmt, &guards)
        };
        let stops_here = dce_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
        for stmt in &dce_stmt {
            invalidate_guards_for_stmt(stmt, &mut guards);
        }
        eliminated.extend(dce_stmt);
        if stops_here {
            break;
        }
        if use_tail_sink {
            break;
        }
    }
    eliminated
}

fn guard_literal_to_scalar(value: &GuardLiteral) -> ScalarValue {
    match value {
        GuardLiteral::Bool(value) => ScalarValue::Bool(*value),
        GuardLiteral::Null => ScalarValue::Null,
        GuardLiteral::Int(value) => ScalarValue::Int(*value),
        GuardLiteral::Float(bits) => ScalarValue::Float(f64::from_bits(*bits)),
        GuardLiteral::String(value) => ScalarValue::String(value.clone()),
    }
}

fn known_scalar_subject_value(subject: &Expr, guards: &GuardState) -> Option<ScalarValue> {
    scalar_value(subject).or_else(|| match &subject.kind {
        ExprKind::Variable(name) => known_exact_guard(guards, name).map(guard_literal_to_scalar),
        _ => None,
    })
}

fn known_subject_truthiness(subject: &Expr, guards: &GuardState) -> Option<bool> {
    if let Some(subject_value) = known_scalar_subject_value(subject, guards) {
        let guard_literal = match subject_value {
            ScalarValue::Bool(value) => GuardLiteral::Bool(value),
            ScalarValue::Null => GuardLiteral::Null,
            ScalarValue::Int(value) => GuardLiteral::Int(value),
            ScalarValue::Float(value) => GuardLiteral::Float(value.to_bits()),
            ScalarValue::String(value) => GuardLiteral::String(value),
        };
        return Some(guard_literal_truthy(&guard_literal));
    }

    let ExprKind::Variable(name) = &subject.kind else {
        return None;
    };

    if guards.bool_true_vars.iter().any(|known| known == name)
        || guards.truthy_vars.iter().any(|known| known == name)
    {
        return Some(true);
    }

    if guards.bool_false_vars.iter().any(|known| known == name)
        || guards.falsy_vars.iter().any(|known| known == name)
    {
        return Some(false);
    }

    None
}

pub(crate) fn dce_stmt(stmt: Stmt) -> Vec<Stmt> {
    dce_stmt_with_guards(stmt, &GuardState::default())
}

fn dce_stmt_with_guards(stmt: Stmt, guards: &GuardState) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => vec![Stmt {
            kind: StmtKind::Echo(prune_expr(expr)),
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
                name,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::TypedAssign {
            name,
            type_expr,
            value,
        } => vec![Stmt {
            kind: StmtKind::TypedAssign {
                name,
                type_expr,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyAssign {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => vec![Stmt {
            kind: StmtKind::StaticPropertyArrayAssign {
                receiver,
                property,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayAssign {
                object: Box::new(prune_expr(*object)),
                property,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayPush {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::ArrayAssign { array, index, value } => vec![Stmt {
            kind: StmtKind::ArrayAssign {
                array,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::ArrayPush { array, value } => vec![Stmt {
            kind: StmtKind::ArrayPush {
                array,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::ListUnpack { vars, value } => vec![Stmt {
            kind: StmtKind::ListUnpack {
                vars,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::StaticVar { name, init } => vec![Stmt {
            kind: StmtKind::StaticVar {
                name,
                init: prune_expr(init),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::ConstDecl { name, value } => vec![Stmt {
            kind: StmtKind::ConstDecl {
                name,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::IncludeOnceMark { label } => vec![Stmt {
            kind: StmtKind::IncludeOnceMark { label },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::IncludeOnceGuard { label, body } => vec![Stmt {
            kind: StmtKind::IncludeOnceGuard {
                label,
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => dce_if_stmt(condition, then_body, elseif_clauses, else_body, span, guards),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = dce_block_with_guards(then_body, guards.clone());
            let else_body =
                normalize_optional_block(else_body.map(|body| dce_block_with_guards(body, guards.clone())));
            if then_body.is_empty() && else_body.is_none() {
                Vec::new()
            } else {
                vec![Stmt {
                    kind: StmtKind::IfDef {
                        symbol,
                        then_body,
                        else_body,
                    },
                    span,
                    attributes: Vec::new(),
                }]
            }
        }
        StmtKind::While { condition, body } => vec![Stmt {
            kind: StmtKind::While {
                condition: prune_expr(condition),
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::DoWhile { body, condition } => vec![Stmt {
            kind: StmtKind::DoWhile {
                body: dce_block_with_guards(body, guards.clone()),
                condition: prune_expr(condition),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => vec![Stmt {
            kind: StmtKind::For {
                init: init.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                condition: condition.map(prune_expr),
                update: update.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => vec![Stmt {
            kind: StmtKind::Foreach {
                array: prune_expr(array),
                key_var,
                value_var,
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt(subject, cases, default, span, guards),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => dce_try_stmt(try_body, catches, finally_body, span, guards),
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => vec![Stmt {
            kind: StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body: dce_block_with_guards(body, GuardState::default()),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Throw(expr) => vec![Stmt {
            kind: StmtKind::Throw(prune_expr(expr)),
            span,
            attributes: Vec::new(),
        }],
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        constants,
        } => {
            let parent_name = extends.as_ref().map(|parent| parent.as_str().to_string());
            let methods = methods
                .into_iter()
                .map(|method| dce_method(method, &name, parent_name.as_deref()))
                .collect();
            vec![Stmt {
                kind: StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    is_abstract,
                    is_final,
                    is_readonly_class,
                    trait_uses,
                    properties,
                    methods,
                constants,
                },
                span,
                attributes: Vec::new(),
            }]
        }
        StmtKind::ExprStmt(expr) => {
            let expr = prune_expr(expr);
            if expr_has_side_effects(&expr) {
                vec![Stmt {
                    kind: StmtKind::ExprStmt(expr),
                    span,
                    attributes: Vec::new(),
                }]
            } else {
                Vec::new()
            }
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => vec![Stmt {
            kind: StmtKind::EnumDecl {
                name,
                backing_type,
                cases,
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::PackedClassDecl { name, fields } => vec![Stmt {
            kind: StmtKind::PackedClassDecl { name, fields },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        constants,
        } => vec![Stmt {
            kind: StmtKind::InterfaceDecl {
                name,
                extends,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            constants,
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        constants,
        } => vec![Stmt {
            kind: StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            constants,
            },
            span,
            attributes: Vec::new(),
        }],
        kind => vec![Stmt { kind, span, attributes: Vec::new() }],
    }
}
