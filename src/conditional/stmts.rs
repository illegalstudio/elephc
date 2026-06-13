//! Purpose:
//! Rewrites statement lists while applying `ifdef` conditions from the CLI.
//! Chooses active conditional branches and recurses through all statement-owned child bodies.
//!
//! Called from:
//! - `crate::conditional::apply()` and `crate::conditional::exprs::rewrite_expr()`.
//!
//! Key details:
//! - Branch removal must happen structurally so later passes never see statements from inactive code.

use std::collections::HashSet;

use crate::parser::ast::{CatchClause, Stmt, StmtKind};

use super::exprs::rewrite_expr;

/// Recursively rewrites a statement list, selecting `ifdef` branches and recursing into child bodies.
///
/// Each `StmtKind::IfDef` is resolved by selecting `then_body` or `else_body` based on whether
/// the symbol is present in `defines`. All other statements have their expressions rewritten
/// and child bodies recursively processed. Returns a new `Vec<Stmt>` with inactive branches removed.
pub(super) fn apply_stmts(stmts: Vec<Stmt>, defines: &HashSet<String>) -> Vec<Stmt> {
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
                result.push(Stmt::with_attributes(
                    rewrite_stmt_kind(other, defines),
                    stmt.span,
                    stmt.attributes,
                ));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Stmt;
    use crate::span::Span;

    /// Builds a `for` header whose init slot holds an `ifdef` — a position the parser
    /// cannot currently produce — and verifies `rewrite_stmt_kind` (reached via the for
    /// header) resolves the ifdef against `defines` instead of hitting `unreachable!()`.
    #[test]
    fn for_header_ifdef_resolves_active_branch_without_panic() {
        let span = Span::new(1, 1);
        let ifdef = Stmt::new(
            StmtKind::IfDef {
                symbol: "FEATURE".to_string(),
                then_body: vec![Stmt::new(StmtKind::Break(0), span)],
                else_body: Some(vec![Stmt::new(StmtKind::Continue(0), span)]),
            },
            span,
        );
        let for_stmt = Stmt::new(
            StmtKind::For {
                init: Some(Box::new(ifdef)),
                condition: None,
                update: None,
                body: Vec::new(),
            },
            span,
        );

        let mut defines = HashSet::new();
        defines.insert("FEATURE".to_string());
        let result = apply_stmts(vec![for_stmt], &defines);

        let StmtKind::For { init: Some(init), .. } = &result[0].kind else {
            panic!("expected a For statement with an init slot");
        };
        let StmtKind::Synthetic(stmts) = &init.kind else {
            panic!("expected the ifdef init to be flattened into a Synthetic block");
        };
        assert!(
            matches!(stmts.as_slice(), [Stmt { kind: StmtKind::Break(0), .. }]),
            "expected the active (then) branch to be selected"
        );
    }
}

/// Rewrites a single `StmtKind` by applying `ifdef` conditions and recursively processing child bodies.
///
/// For each variant, expressions are rewritten via `rewrite_expr` and nested statement lists are
/// rewritten via `apply_stmts`. Branchless variants (declarations, breaks, etc.) are returned unchanged.
/// `IfDef` variants are normally flattened in `apply_stmts` before this is called; if one still
/// reaches here it is resolved against `defines` defensively rather than panicking.
fn rewrite_stmt_kind(kind: StmtKind, defines: &HashSet<String>) -> StmtKind {
    match kind {
        StmtKind::Synthetic(stmts) => StmtKind::Synthetic(apply_stmts(stmts, defines)),
        StmtKind::IncludeOnceMark { label } => StmtKind::IncludeOnceMark { label },
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: apply_stmts(body, defines),
        },
        StmtKind::Echo(expr) => StmtKind::Echo(rewrite_expr(expr, defines)),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: rewrite_expr(value, defines),
        },
        StmtKind::RefAssign { target, source } => StmtKind::RefAssign { target, source },
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
        StmtKind::NestedArrayAssign { target, value } => StmtKind::NestedArrayAssign {
            target: rewrite_expr(target, defines),
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
            value_by_ref,
            body,
        } => StmtKind::Foreach {
            array: rewrite_expr(array, defines),
            key_var,
            value_var,
            value_by_ref,
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
        StmtKind::Break(levels) => StmtKind::Break(levels),
        StmtKind::Continue(levels) => StmtKind::Continue(levels),
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(rewrite_expr(expr, defines)),
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => StmtKind::FunctionDecl {
            name,
            params: params
                .into_iter()
                .map(|(name, type_ann, default, is_ref)| {
                    (name, type_ann, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                })
                .collect(),
            variadic,
            return_type,
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
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        constants,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties: properties
                .into_iter()
                .map(|mut property| {
                    property.default =
                        property.default.map(|expr| rewrite_expr(expr, defines));
                    property
                })
                .collect(),
            methods: methods
                .into_iter()
                .map(|mut method| {
                    method.params = method
                        .params
                        .into_iter()
                        .map(|(name, type_ann, default, is_ref)| {
                            (name, type_ann, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                        })
                        .collect();
                    method.body = apply_stmts(method.body, defines);
                    method
                })
                .collect(),
        constants,
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
            implements,
            methods,
            constants,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            implements,
            methods,
            constants,
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.value = case.value.map(|expr| rewrite_expr(expr, defines));
                    case
                })
                .collect(),
        },
        StmtKind::InterfaceDecl {
            name,
            extends,
            properties,
            methods,
        constants,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            properties: properties
                .into_iter()
                .map(|mut property| {
                    property.default =
                        property.default.map(|expr| rewrite_expr(expr, defines));
                    property
                })
                .collect(),
            methods,
        constants,
        },
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        constants,
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
                        .map(|(name, type_ann, default, is_ref)| {
                            (name, type_ann, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                        })
                        .collect();
                    method.body = apply_stmts(method.body, defines);
                    method
                })
                .collect(),
        constants,
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
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value: rewrite_expr(value, defines),
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value: rewrite_expr(value, defines),
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index: rewrite_expr(index, defines),
            value: rewrite_expr(value, defines),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(rewrite_expr(*object, defines)),
            property,
            value: rewrite_expr(value, defines),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(rewrite_expr(*object, defines)),
            property,
            index: rewrite_expr(index, defines),
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
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            // Defense-in-depth: `apply_stmts` normally flattens ifdefs before this runs,
            // so this arm is unreachable today. If a future caller routes an ifdef here
            // (e.g. a `for` header), resolve it against `defines` like `apply_stmts` does
            // rather than panicking, so the invariant fails soft instead of crashing.
            let selected = if defines.contains(&symbol) {
                then_body
            } else {
                else_body.unwrap_or_default()
            };
            StmtKind::Synthetic(apply_stmts(selected, defines))
        }
        StmtKind::NamespaceDecl { name } => StmtKind::NamespaceDecl { name },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
            name,
            body: apply_stmts(body, defines),
        },
        StmtKind::UseDecl { imports } => StmtKind::UseDecl { imports },
        StmtKind::PackedClassDecl { name, fields } => StmtKind::PackedClassDecl { name, fields },
        StmtKind::FunctionVariantGroup { name, variants } => {
            StmtKind::FunctionVariantGroup { name, variants }
        }
        StmtKind::FunctionVariantMark { name, variant } => {
            StmtKind::FunctionVariantMark { name, variant }
        }
    }
}
