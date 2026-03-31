use std::collections::HashSet;

use crate::parser::ast::{CatchClause, Expr, ExprKind, Program, Stmt, StmtKind};

pub fn apply(program: Program, defines: &HashSet<String>) -> Program {
    apply_stmts(program, defines)
}

fn apply_stmts(stmts: Vec<Stmt>, defines: &HashSet<String>) -> Vec<Stmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        match stmt.kind {
            StmtKind::IfDef {
                symbol,
                then_body,
                else_body,
            } => {
                let selected = if defines.contains(&symbol) {
                    then_body
                } else {
                    else_body.unwrap_or_default()
                };
                result.extend(apply_stmts(selected, defines));
            }
            other => {
                result.push(Stmt::new(rewrite_stmt_kind(other, defines), stmt.span));
            }
        }
    }
    result
}

fn rewrite_stmt_kind(kind: StmtKind, defines: &HashSet<String>) -> StmtKind {
    match kind {
        StmtKind::Echo(expr) => StmtKind::Echo(rewrite_expr(expr, defines)),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: rewrite_expr(value, defines),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: rewrite_expr(condition, defines),
            then_body: apply_stmts(then_body, defines),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(cond, body)| (rewrite_expr(cond, defines), apply_stmts(body, defines)))
                .collect(),
            else_body: else_body.map(|body| apply_stmts(body, defines)),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: rewrite_expr(condition, defines),
            body: apply_stmts(body, defines),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: apply_stmts(body, defines),
            condition: rewrite_expr(condition, defines),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|stmt| Box::new(Stmt::new(rewrite_stmt_kind(stmt.kind, defines), stmt.span))),
            condition: condition.map(|expr| rewrite_expr(expr, defines)),
            update: update.map(|stmt| Box::new(Stmt::new(rewrite_stmt_kind(stmt.kind, defines), stmt.span))),
            body: apply_stmts(body, defines),
        },
        StmtKind::ArrayAssign { array, index, value } => StmtKind::ArrayAssign {
            array,
            index: rewrite_expr(index, defines),
            value: rewrite_expr(value, defines),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: rewrite_expr(value, defines),
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: rewrite_expr(value, defines),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: rewrite_expr(array, defines),
            key_var,
            value_var,
            body: apply_stmts(body, defines),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: rewrite_expr(subject, defines),
            cases: cases
                .into_iter()
                .map(|(values, body)| {
                    (
                        values
                            .into_iter()
                            .map(|expr| rewrite_expr(expr, defines))
                            .collect(),
                        apply_stmts(body, defines),
                    )
                })
                .collect(),
            default: default.map(|body| apply_stmts(body, defines)),
        },
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path,
            once,
            required,
        },
        StmtKind::Throw(expr) => StmtKind::Throw(rewrite_expr(expr, defines)),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: apply_stmts(try_body, defines),
            catches: catches
                .into_iter()
                .map(|catch_clause| CatchClause {
                    exception_types: catch_clause.exception_types,
                    variable: catch_clause.variable,
                    body: apply_stmts(catch_clause.body, defines),
                })
                .collect(),
            finally_body: finally_body.map(|body| apply_stmts(body, defines)),
        },
        StmtKind::Break => StmtKind::Break,
        StmtKind::Continue => StmtKind::Continue,
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(rewrite_expr(expr, defines)),
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            body,
        } => StmtKind::FunctionDecl {
            name,
            params: params
                .into_iter()
                .map(|(name, default, is_ref)| {
                    (name, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                })
                .collect(),
            variadic,
            body: apply_stmts(body, defines),
        },
        StmtKind::Return(expr) => StmtKind::Return(expr.map(|expr| rewrite_expr(expr, defines))),
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: rewrite_expr(value, defines),
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: rewrite_expr(value, defines),
        },
        StmtKind::Global { vars } => StmtKind::Global { vars },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: rewrite_expr(init, defines),
        },
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            trait_uses,
            properties,
            methods,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            trait_uses,
            properties,
            methods: methods
                .into_iter()
                .map(|mut method| {
                    method.params = method
                        .params
                        .into_iter()
                        .map(|(name, default, is_ref)| {
                            (name, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                        })
                        .collect();
                    method.body = apply_stmts(method.body, defines);
                    method
                })
                .collect(),
        },
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        },
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods: methods
                .into_iter()
                .map(|mut method| {
                    method.params = method
                        .params
                        .into_iter()
                        .map(|(name, default, is_ref)| {
                            (name, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                        })
                        .collect();
                    method.body = apply_stmts(method.body, defines);
                    method
                })
                .collect(),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(rewrite_expr(*object, defines)),
            property,
            value: rewrite_expr(value, defines),
        },
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        },
        StmtKind::ExternClassDecl { name, fields } => StmtKind::ExternClassDecl { name, fields },
        StmtKind::ExternGlobalDecl { name, c_type } => {
            StmtKind::ExternGlobalDecl { name, c_type }
        }
        StmtKind::IfDef { .. } => unreachable!("ifdefs are flattened in apply_stmts"),
        StmtKind::NamespaceDecl { name } => StmtKind::NamespaceDecl { name },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
            name,
            body: apply_stmts(body, defines),
        },
        StmtKind::UseDecl { imports } => StmtKind::UseDecl { imports },
        StmtKind::PackedClassDecl { name, fields } => StmtKind::PackedClassDecl { name, fields },
    }
}

fn rewrite_expr(expr: Expr, defines: &HashSet<String>) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(rewrite_expr(*left, defines)),
            op,
            right: Box::new(rewrite_expr(*right, defines)),
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(rewrite_expr(*value, defines)),
            default: Box::new(rewrite_expr(*default, defines)),
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::ArrayLiteral(elems) => ExprKind::ArrayLiteral(
            elems
                .into_iter()
                .map(|elem| rewrite_expr(elem, defines))
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(entries) => ExprKind::ArrayLiteralAssoc(
            entries
                .into_iter()
                .map(|(key, value)| (rewrite_expr(key, defines), rewrite_expr(value, defines)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(rewrite_expr(*subject, defines)),
            arms: arms
                .into_iter()
                .map(|(values, expr)| {
                    (
                        values
                            .into_iter()
                            .map(|value| rewrite_expr(value, defines))
                            .collect(),
                        rewrite_expr(expr, defines),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(rewrite_expr(*expr, defines))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(rewrite_expr(*array, defines)),
            index: Box::new(rewrite_expr(*index, defines)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(rewrite_expr(*condition, defines)),
            then_expr: Box::new(rewrite_expr(*then_expr, defines)),
            else_expr: Box::new(rewrite_expr(*else_expr, defines)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(rewrite_expr(*expr, defines)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params: params
                .into_iter()
                .map(|(name, default, is_ref)| {
                    (name, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                })
                .collect(),
            variadic,
            body: apply_stmts(body, defines),
            is_arrow,
            captures,
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(rewrite_expr(*callee, defines)),
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(rewrite_expr(*object, defines)),
            property,
        },
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(rewrite_expr(*object, defines)),
            method,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(rewrite_expr(*expr, defines)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(rewrite_expr(*len, defines)),
        },
        other => other,
    };
    Expr::new(kind, span)
}
