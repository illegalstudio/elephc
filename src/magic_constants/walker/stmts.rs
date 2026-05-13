//! Purpose:
//! Walks statement AST nodes for magic-constant substitution passes.
//! Rebuilds programs, control-flow bodies, declarations, and include expressions through pass hooks.
//!
//! Called from:
//! - `crate::magic_constants::walker::walk_program()`.
//!
//! Key details:
//! - Scope-bearing statements must enter and exit pass context in PHP lexical order.

use crate::parser::ast::{CatchClause, EnumCaseDecl, Stmt, StmtKind};

use super::exprs::walk_expr;
use super::members::{walk_class_method, walk_class_property};
use super::Pass;

pub(in crate::magic_constants) fn walk_program<P: Pass>(stmts: Vec<Stmt>, pass: &mut P) -> Vec<Stmt> {
    stmts.into_iter().map(|s| walk_stmt(s, pass)).collect()
}

pub(super) fn walk_stmt<P: Pass>(stmt: Stmt, pass: &mut P) -> Stmt {
    let span = stmt.span;
    let attributes = stmt.attributes.clone();
    let kind = match stmt.kind {
        StmtKind::Synthetic(stmts) => StmtKind::Synthetic(walk_program(stmts, pass)),
        StmtKind::IncludeOnceMark { label } => StmtKind::IncludeOnceMark { label },
        StmtKind::FunctionVariantGroup { name, variants } => {
            StmtKind::FunctionVariantGroup { name, variants }
        }
        StmtKind::FunctionVariantMark { name, variant } => {
            StmtKind::FunctionVariantMark { name, variant }
        }
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: walk_program(body, pass),
        },
        StmtKind::Echo(e) => StmtKind::Echo(walk_expr(e, pass)),
        StmtKind::Throw(e) => StmtKind::Throw(walk_expr(e, pass)),
        StmtKind::ExprStmt(e) => StmtKind::ExprStmt(walk_expr(e, pass)),
        StmtKind::Return(e) => StmtKind::Return(e.map(|x| walk_expr(x, pass))),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: walk_expr(init, pass),
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(walk_expr(*object, pass)),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(walk_expr(*object, pass)),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(walk_expr(*object, pass)),
            property,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: walk_expr(condition, pass),
            then_body: walk_program(then_body, pass),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(c, b)| (walk_expr(c, pass), walk_program(b, pass)))
                .collect(),
            else_body: else_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => StmtKind::IfDef {
            symbol,
            then_body: walk_program(then_body, pass),
            else_body: else_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: walk_expr(condition, pass),
            body: walk_program(body, pass),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: walk_program(body, pass),
            condition: walk_expr(condition, pass),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|s| Box::new(walk_stmt(*s, pass))),
            condition: condition.map(|e| walk_expr(e, pass)),
            update: update.map(|s| Box::new(walk_stmt(*s, pass))),
            body: walk_program(body, pass),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: walk_expr(array, pass),
            key_var,
            value_var,
            body: walk_program(body, pass),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: walk_expr(subject, pass),
            cases: cases
                .into_iter()
                .map(|(patterns, body)| {
                    (
                        patterns.into_iter().map(|e| walk_expr(e, pass)).collect(),
                        walk_program(body, pass),
                    )
                })
                .collect(),
            default: default.map(|b| walk_program(b, pass)),
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: walk_program(try_body, pass),
            catches: catches
                .into_iter()
                .map(|c| CatchClause {
                    exception_types: c.exception_types,
                    variable: c.variable,
                    body: walk_program(c.body, pass),
                })
                .collect(),
            finally_body: finally_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => {
            pass.enter_function(&name);
            let new_params = params
                .into_iter()
                .map(|(n, t, default, by_ref)| {
                    (n, t, default.map(|d| walk_expr(d, pass)), by_ref)
                })
                .collect();
            let new_body = walk_program(body, pass);
            pass.leave_function();
            StmtKind::FunctionDecl {
                name,
                params: new_params,
                variadic,
                return_type,
                body: new_body,
            }
        }
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
            pass.enter_class(&name);
            let new_properties = properties
                .into_iter()
                .map(|p| walk_class_property(p, pass))
                .collect();
            let new_methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            pass.leave_class();
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                is_abstract,
                is_final,
                is_readonly_class,
                trait_uses,
                properties: new_properties,
                methods: new_methods,
            constants,
            }
        }
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        constants,
        } => {
            pass.enter_trait(&name);
            let new_properties = properties
                .into_iter()
                .map(|p| walk_class_property(p, pass))
                .collect();
            let new_methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            pass.leave_trait();
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties: new_properties,
                methods: new_methods,
            constants,
            }
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        constants,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods: methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect(),
        constants,
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            cases: cases
                .into_iter()
                .map(|case| EnumCaseDecl {
                    name: case.name,
                    value: case.value.map(|e| walk_expr(e, pass)),
                    span: case.span,
                    attributes: case.attributes,
                })
                .collect(),
        },
        StmtKind::NamespaceDecl { name } => {
            pass.enter_namespace_decl(&name);
            StmtKind::NamespaceDecl { name }
        }
        StmtKind::NamespaceBlock { name, body } => {
            pass.enter_namespace_block(&name);
            let new_body = walk_program(body, pass);
            pass.leave_namespace_block();
            StmtKind::NamespaceBlock {
                name,
                body: new_body,
            }
        }
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path: walk_expr(path, pass),
            once,
            required,
        },
        // Statements with no Expr children or only simple data:
        other @ (StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }) => other,
    };
    Stmt { kind, span, attributes }
}
